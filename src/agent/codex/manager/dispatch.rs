//! Routing responses, server requests, and connection notifications.

use std::sync::{Arc, atomic::Ordering};

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Value, json};

use super::super::{
    client_tools::{TOOL_CALL_METHOD, parse_dynamic_tool_call_request},
    methods::is_unintegrated_server_notification,
    server_requests::{
        ControlledServerRequestReply, controlled_reply_message, invalid_params_reply,
        reply_to_controlled_server_request, reply_to_dynamic_tool_call,
        unintegrated_notification_diagnostic,
    },
};
use super::{
    super::{
        TURN_SCOPED_SERVER_METHODS, ensure_server_method_is_defined,
        is_integrated_server_request_method, parse_agent_notification,
        parse_mcp_server_startup_status_updated, parse_thread_status_changed,
        request_id_from_value, thread_started_id, validate_remote_control_status_changed,
    },
    ManagerInner,
    connection::{Connection, ServerRequestResponder, ThreadLifecycleKind, TurnKey, TurnRoute},
    protocol::{
        optional_nullable_param_string, required_nullable_param_string, required_param_string,
    },
    turn::turn_id_from_turn_message,
};
use crate::agent::{AgentConnectionEvent, AgentEvent, ProjectChange};

impl ManagerInner {
    pub(super) fn handle_message(
        &self,
        connection: &Arc<Connection>,
        message: &Value,
    ) -> Result<()> {
        if connection.failed.load(Ordering::Acquire) {
            return Ok(());
        }
        if message.get("method").is_none() {
            return connection.handle_response(message.clone());
        }
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .context("Codex JSON-RPC method 必须是字符串")?;
        if message.get("id").is_some() {
            return self.handle_server_request(connection, method, message);
        }
        self.handle_notification(connection, method, message)
    }
    pub(super) fn handle_server_request(
        &self,
        connection: &Arc<Connection>,
        method: &str,
        message: &Value,
    ) -> Result<()> {
        if is_integrated_server_request_method(method) {
            return self.handle_interactive_server_request(connection, method, message);
        }
        if method == TOOL_CALL_METHOD {
            return self.handle_client_tool_call_request(connection, message);
        }
        // Everything else is answered under its original id: the legacy approval
        // protocols, methods this client deliberately does not integrate, and any
        // method outside the schema this client was built against. The connection
        // and every active turn keep running; the old guardrail of failing the
        // generation to make an uncovered method visible now costs shared pending
        // RPCs and live turns, so visibility comes from the diagnostic trail.
        let reply = match reply_to_controlled_server_request(method, message, &self.client_tools) {
            Ok(reply) => reply,
            Err(_error) => return self.answer_invalid_params(connection, method, message),
        };
        // A duplicate id would answer two requests under one wire id; that stays
        // the same protocol error it is for every other server request.
        connection.record_controlled_server_request(&reply.diagnostic.request_id)?;
        self.write_controlled_reply(connection, reply)
    }

    /// Server requests with a real interactive responder: v2 approvals, user
    /// input, permissions, and MCP elicitation. Their validation and fatal-error
    /// semantics are unchanged.
    fn handle_interactive_server_request(
        &self,
        connection: &Arc<Connection>,
        method: &str,
        message: &Value,
    ) -> Result<()> {
        // MCP elicitation is a standalone server-to-client request: it is owned
        // by the connection generation, not by a turn, and must stay answerable
        // while no turn is active.
        if method == super::super::elicitation::ELICITATION_METHOD {
            return self.handle_mcp_elicitation_request(connection, message);
        }
        let thread_id = message
            .pointer("/params/threadId")
            .and_then(Value::as_str)
            .context("server request 缺少字符串 params.threadId")?;
        let turn_id = message
            .pointer("/params/turnId")
            .and_then(Value::as_str)
            .context("server request 缺少字符串 params.turnId")?;
        if !connection.has_pending_start(thread_id)?
            && connection
                .turn_for_key(&TurnKey {
                    thread_id: thread_id.to_owned(),
                    turn_id: turn_id.to_owned(),
                })
                .is_err()
        {
            bail!("收到未知 turn 的消息：threadId=`{thread_id}`，turnId=`{turn_id}`");
        }
        let request_id = request_id_from_value(
            message
                .get("id")
                .context("server request 缺少 JSON-RPC id")?,
        )?;
        let key = TurnKey {
            thread_id: thread_id.to_owned(),
            turn_id: turn_id.to_owned(),
        };
        connection.record_server_request_owner(
            request_id,
            key,
            ServerRequestResponder::Interactive,
        )?;
        // The owner is recorded first so a resolution that races the buffer
        // still finds it.
        match connection.route(thread_id, turn_id, message)? {
            TurnRoute::Owned(turn) => {
                if let Some(outcome) = turn.ingest(message)? {
                    connection.finish_turn(&turn, Ok(outcome));
                }
            }
            TurnRoute::Buffered => {}
            TurnRoute::Unowned => {
                bail!("收到未知 turn 的消息：threadId=`{thread_id}`，turnId=`{turn_id}`")
            }
        }
        Ok(())
    }

    /// `item/tool/call` is turn-scoped like every other turn request and follows
    /// the same thread/turn routing and pending `turn/start` rules, but it is
    /// answered immediately under its original id: this client shows no tool card,
    /// and the model needs a result to continue its turn. The ownership record is
    /// released by the matching `serverRequest/resolved`; duplicates and late
    /// resolutions stay inert.
    fn handle_client_tool_call_request(
        &self,
        connection: &Arc<Connection>,
        message: &Value,
    ) -> Result<()> {
        let call = match parse_dynamic_tool_call_request(message) {
            Ok(call) => call,
            Err(_error) => {
                return self.answer_invalid_params(connection, TOOL_CALL_METHOD, message);
            }
        };
        if !connection.has_pending_start(&call.thread_id)?
            && connection
                .turn_for_key(&TurnKey {
                    thread_id: call.thread_id.clone(),
                    turn_id: call.turn_id.clone(),
                })
                .is_err()
        {
            bail!(
                "收到未知 turn 的消息：threadId=`{}`，turnId=`{}`",
                call.thread_id,
                call.turn_id
            );
        }
        connection.record_server_request_owner(
            call.request_id.clone(),
            TurnKey {
                thread_id: call.thread_id.clone(),
                turn_id: call.turn_id.clone(),
            },
            ServerRequestResponder::ControlledReply,
        )?;
        let reply = reply_to_dynamic_tool_call(&call, &self.client_tools, message);
        self.write_controlled_reply(connection, reply)
    }

    /// Writes one controlled reply under the request's original id, keeping the
    /// string/number distinction, and records its diagnostic.
    fn write_controlled_reply(
        &self,
        connection: &Arc<Connection>,
        reply: ControlledServerRequestReply,
    ) -> Result<()> {
        connection.send_message(controlled_reply_message(&reply))?;
        connection.record_server_request_diagnostic(reply.diagnostic);
        Ok(())
    }

    /// `-32602` for a controlled request whose payload this client could not
    /// decode. The connection and the active turn stay alive: for an
    /// auto-answered method a malformed payload is a request-level error, not a
    /// fatal protocol error.
    fn answer_invalid_params(
        &self,
        connection: &Arc<Connection>,
        method: &str,
        message: &Value,
    ) -> Result<()> {
        let request_id = request_id_from_value(
            message
                .get("id")
                .context("server request 缺少 JSON-RPC id")?,
        )?;
        // The id is remembered even though the payload failed validation: the
        // server may still resolve this request, and a resolution must stay inert
        // instead of failing the generation.
        connection.record_controlled_server_request(&request_id)?;
        let reply = invalid_params_reply(method, message, request_id);
        self.write_controlled_reply(connection, reply)
    }

    fn handle_mcp_elicitation_request(
        &self,
        connection: &Arc<Connection>,
        message: &Value,
    ) -> Result<()> {
        let request = match super::super::elicitation::parse_mcp_server_elicitation_request(
            message,
            connection.generation,
        ) {
            Ok(request) => request,
            Err(error) => {
                // Unsupported modes and malformed params follow the same
                // failure semantics as an unknown server request: answer under
                // the original id, then terminate the generation.
                connection.send_message(json!({
                    "id": message.get("id").cloned().unwrap_or(Value::Null),
                    "error": {
                        "code": -32602,
                        "message": "Invalid mcpServer/elicitation/request params"
                    }
                }))?;
                return Err(error).context("mcpServer/elicitation/request 参数校验失败");
            }
        };
        if let Err(error) = connection.register_mcp_elicitation(request.clone()) {
            // A duplicate id already identifies another responder; never send an
            // error under that same id and accidentally answer the original.
            return Err(error.context("mcpServer/elicitation/request 注册失败"));
        }
        let responder = connection.mcp_elicitation_handle(request.identity());
        self.publish_connection_event(AgentConnectionEvent::McpElicitationRequested {
            request,
            responder,
        });
        Ok(())
    }
    pub(super) fn handle_notification(
        &self,
        connection: &Arc<Connection>,
        method: &str,
        message: &Value,
    ) -> Result<()> {
        match method {
            "item/started" | "item/completed"
                if message.pointer("/params/item/type").and_then(Value::as_str)
                    == Some("hookPrompt") =>
            {
                let thread_id = required_param_string(message, "threadId", method)?;
                let turn_id = required_param_string(message, "turnId", method)?;
                let prompt = super::super::runtime::parse_hook_prompt(
                    message
                        .pointer("/params/item")
                        .context("hookPrompt 缺少 item")?,
                    Some(method == "item/completed"),
                )
                .map_err(|error| super::super::items::turn_item_protocol_error(message, error))?;
                let active = connection
                    .state
                    .lock()
                    .map_err(|_| anyhow!("connection state 锁已损坏"))?
                    .turns
                    .get(&TurnKey {
                        thread_id: thread_id.clone(),
                        turn_id: turn_id.clone(),
                    })
                    .cloned();
                if let Some(active) = active
                    && active
                        .dispatch
                        .lock()
                        .map_err(|_| anyhow!("turn dispatch 锁已损坏"))?
                        .accepted
                {
                    // Preserve item order on the accepted turn's existing channel.
                    // The observation snapshot remains authoritative for early/late replay.
                    let _ = active
                        .events
                        .send_blocking(AgentEvent::HookPromptUpdated(prompt.clone()));
                }
                self.publish_runtime(
                    connection.generation,
                    crate::agent::AgentRuntimeObservation::HookPrompt(
                        crate::agent::AgentScopedHookPrompt {
                            thread_id,
                            turn_id,
                            prompt,
                        },
                    ),
                )
            }
            "deprecationNotice" => {
                self.publish_connection_event(AgentConnectionEvent::DeprecationNotice(
                    super::super::runtime::parse_deprecation(message)?,
                ));
                Ok(())
            }
            method if super::super::runtime::RUNTIME_METHODS.contains(&method) => self
                .publish_runtime(
                    connection.generation,
                    super::super::runtime::parse_runtime(message)?,
                ),
            "guardianWarning" => {
                self.publish_connection_event(AgentConnectionEvent::GuardianWarning(
                    super::super::auto_approval::parse_guardian_warning(message)?,
                ));
                Ok(())
            }
            method if super::super::auto_approval::REVIEW_METHODS.contains(&method) => {
                // Review observations outlive the turn event channel. Route all
                // of them by their explicit identity through the thread hub;
                // they must never bind a pending turn/start or race its cleanup.
                let event = parse_agent_notification(message)?.context("review event 缺失")?;
                match event {
                    AgentEvent::AutoApprovalReviewUpdated(review) => self.publish_connection_event(
                        AgentConnectionEvent::AutoApprovalReviewUpdated(review),
                    ),
                    AgentEvent::StrictReviewRequired(requirement) => self.publish_connection_event(
                        AgentConnectionEvent::StrictReviewRequired(requirement),
                    ),
                    _ => unreachable!("REVIEW_METHODS maps only review observations"),
                }
                Ok(())
            }
            "thread/started" => self.handle_thread_started(connection, message),
            super::super::goal::GOAL_UPDATED_METHOD | super::super::goal::GOAL_CLEARED_METHOD => {
                let event = match super::super::goal::parse_notification(message)? {
                    super::super::goal::GoalNotification::Updated {
                        thread_id,
                        turn_id,
                        goal,
                    } => AgentConnectionEvent::ThreadGoalUpdated {
                        generation: connection.generation,
                        thread_id,
                        turn_id,
                        goal,
                    },
                    super::super::goal::GoalNotification::Cleared { thread_id } => {
                        AgentConnectionEvent::ThreadGoalCleared {
                            generation: connection.generation,
                            thread_id,
                        }
                    }
                };
                self.publish_connection_event(event);
                Ok(())
            }
            super::super::queue::QUEUE_CHANGED_METHOD => {
                let thread_id = super::super::queue::parse_changed(message)?;
                self.publish_connection_event(AgentConnectionEvent::ThreadQueueChanged {
                    generation: connection.generation,
                    thread_id,
                });
                Ok(())
            }
            "project/changed" => {
                let project_id = required_param_string(message, "projectId", method)?;
                let change = match required_param_string(message, "changeType", method)?.as_str() {
                    "created" => ProjectChange::Created,
                    "updated" => ProjectChange::Updated,
                    "deleted" => ProjectChange::Deleted,
                    value => bail!("project/changed 的 changeType 为未知值 `{value}`"),
                };
                self.publish_connection_event(AgentConnectionEvent::ProjectChanged {
                    project_id,
                    change,
                });
                Ok(())
            }
            "thread/archived" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                self.publish_connection_event(AgentConnectionEvent::ThreadArchived { thread_id });
                Ok(())
            }
            "thread/unarchived" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                self.publish_connection_event(AgentConnectionEvent::ThreadUnarchived { thread_id });
                Ok(())
            }
            "thread/deleted" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                if let Ok(mut state) = connection.state.lock() {
                    state.loaded_threads.remove(&thread_id);
                }
                self.publish_connection_event(AgentConnectionEvent::ThreadDeleted { thread_id });
                Ok(())
            }
            "thread/name/updated" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                let name = optional_nullable_param_string(message, "threadName", method)?;
                self.publish_connection_event(AgentConnectionEvent::ThreadNameUpdated {
                    thread_id,
                    name,
                });
                Ok(())
            }
            "thread/closed" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                if let Ok(mut state) = connection.state.lock() {
                    state.loaded_threads.remove(&thread_id);
                    state.thread_settings.remove(&thread_id);
                    if let Some(waiter) = state.settings_waiters.remove(&thread_id) {
                        let _ = waiter
                            .sender
                            .try_send(Err("线程已关闭，权限更新未确认".into()));
                    }
                }
                for elicitation in connection.invalidate_mcp_elicitations(Some(&thread_id)) {
                    self.publish_connection_event(AgentConnectionEvent::McpElicitationFailed {
                        identity: elicitation.identity,
                        thread_id: elicitation.thread_id,
                        kind: crate::agent::AgentServerRequestFailureKind::Cancelled,
                        message: "线程已关闭，等待中的 MCP elicitation 不再可回复".to_owned(),
                    });
                }
                self.publish_runtime(
                    connection.generation,
                    crate::agent::AgentRuntimeObservation::ThreadClosed {
                        thread_id: thread_id.clone(),
                    },
                )?;
                self.publish_connection_event(AgentConnectionEvent::ThreadClosed { thread_id });
                Ok(())
            }
            "thread/project/updated" => {
                let thread_id = required_param_string(message, "threadId", method)?;
                let project_id = required_nullable_param_string(message, "projectId", method)?;
                self.publish_connection_event(AgentConnectionEvent::ThreadProjectUpdated {
                    thread_id,
                    project_id,
                });
                Ok(())
            }
            "thread/reverted" => self.handle_thread_reverted(connection, message),
            "fuzzyFileSearch/sessionUpdated" | "fuzzyFileSearch/sessionCompleted" => {
                self.handle_file_search_notification(connection, method, message)
            }
            "serverRequest/resolved" => {
                let request_id = request_id_from_value(
                    message
                        .pointer("/params/requestId")
                        .context("serverRequest/resolved 缺少 params.requestId")?,
                )?;
                let notification_thread = message
                    .pointer("/params/threadId")
                    .and_then(Value::as_str)
                    .context("serverRequest/resolved 缺少字符串 params.threadId")?;
                if connection.knows_mcp_elicitation(&request_id)? {
                    let Some(identity) =
                        connection.resolve_mcp_elicitation(&request_id, notification_thread)?
                    else {
                        // Duplicate or late resolution of an already finished or
                        // invalidated elicitation stays idempotent.
                        return Ok(());
                    };
                    self.publish_connection_event(AgentConnectionEvent::McpElicitationResolved {
                        identity,
                        thread_id: notification_thread.to_owned(),
                    });
                    return Ok(());
                }
                if connection.knows_controlled_server_request(&request_id)? {
                    // This generation answered the request itself under its
                    // original id; there is no responder left to release.
                    return Ok(());
                }
                let Some(owner) =
                    connection.resolve_server_request_owner(&request_id, notification_thread)?
                else {
                    return Ok(());
                };
                if owner.responder == ServerRequestResponder::ControlledReply {
                    // The controlled reply was written when the request arrived;
                    // resolution only releases the ownership record.
                    return Ok(());
                }
                let turn =
                    match connection.route(&owner.key.thread_id, &owner.key.turn_id, message)? {
                        TurnRoute::Owned(turn) => turn,
                        TurnRoute::Buffered => return Ok(()),
                        TurnRoute::Unowned => connection.turn_for_key(&owner.key)?,
                    };
                let mut dispatch = turn
                    .dispatch
                    .lock()
                    .map_err(|_| anyhow!("Codex managed turn dispatch 锁已损坏"))?;
                if !dispatch.accepted {
                    dispatch.buffered.push(message.clone());
                    return Ok(());
                }
                super::super::handle_server_request_resolved(&turn.session, message, &turn.events)
            }
            "thread/settings/updated" => {
                let thread_id = message
                    .pointer("/params/threadId")
                    .and_then(Value::as_str)
                    .context("thread/settings/updated 缺少字符串 params.threadId")?
                    .to_owned();
                if self
                    .temporary_threads
                    .lock()
                    .map_err(|_| anyhow!("临时聊天状态不可用"))?
                    .get(&thread_id)
                    .is_some_and(|thread| {
                        thread.closed || thread.generation != connection.generation
                    })
                {
                    return Ok(());
                }
                let Some(AgentEvent::ThreadSettingsUpdated(settings)) =
                    parse_agent_notification(message)?
                else {
                    bail!("thread/settings/updated 未映射为 AgentThreadSettings");
                };
                {
                    let mut state = connection
                        .state
                        .lock()
                        .map_err(|_| anyhow!("连接状态不可用"))?;
                    if let Some(waiter) = state.settings_waiters.get_mut(&thread_id) {
                        if waiter.observed.is_none()
                            && super::settings::settings_match(&waiter.expected, &settings)
                        {
                            waiter.observed = Some(settings.clone());
                            let _ = waiter.sender.try_send(Ok(settings));
                        }
                        // A pending operation is published only after its own RPC
                        // also succeeds. Mismatched/duplicate receipts stay isolated.
                        return Ok(());
                    }
                    if state
                        .confirmed_settings
                        .get(&thread_id)
                        .is_some_and(|history| history.contains(&settings))
                    {
                        return Ok(());
                    }
                }
                connection
                    .state
                    .lock()
                    .map_err(|_| anyhow!("连接状态不可用"))?
                    .thread_settings
                    .insert(thread_id.clone(), settings.clone());
                self.publish_connection_event(AgentConnectionEvent::ThreadSettingsUpdated {
                    thread_id,
                    generation: connection.generation,
                    settings,
                });
                Ok(())
            }
            "mcpServer/startupStatus/updated" => {
                let status = parse_mcp_server_startup_status_updated(message)?;
                self.publish_connection_event(AgentConnectionEvent::McpServerStartupStatusUpdated(
                    crate::agent::AgentMcpStartupStatusUpdated {
                        generation: connection.generation,
                        status,
                    },
                ));
                Ok(())
            }
            // Watched skill files changed. This is an invalidation signal only;
            // the caller re-reads `skills/list` with its own parameters.
            "skills/changed" => {
                super::super::skills::parse_changed(message)?;
                self.publish_connection_event(AgentConnectionEvent::SkillsChanged {
                    generation: connection.generation,
                });
                Ok(())
            }
            // The app catalog changed. The notification payload is decoded (so
            // a shape change still fails loudly) and then discarded: the
            // directory is re-read through `app/list`, which keeps the client
            // from overwriting a newer local read or clearing an operation that
            // is still in flight.
            "app/list/updated" => {
                super::super::apps::validate_list_updated(message)?;
                self.publish_connection_event(AgentConnectionEvent::AppListUpdated {
                    generation: connection.generation,
                });
                Ok(())
            }
            "mcpServer/oauthLogin/completed" => {
                let notification = super::super::mcp::parse_oauth_completed(message)?;
                // A `None` result means the completion is late, superseded, or
                // was never started by this client: decoded, then inert.
                if let Some(completion) =
                    super::mcp::correlate_oauth_completion(connection, &notification)?
                {
                    connection.publish_oauth_completion(completion);
                }
                Ok(())
            }
            "thread/tokenUsage/updated" => {
                // Resume can publish the saved turn's usage before its RPC
                // response, without any locally running ManagedTurn.
                let Some(AgentEvent::ThreadTokenUsageUpdated(usage)) =
                    parse_agent_notification(message)?
                else {
                    bail!("thread/tokenUsage/updated 未映射为 token usage");
                };
                self.publish_connection_event(AgentConnectionEvent::ThreadTokenUsageUpdated(usage));
                Ok(())
            }
            "thread/status/changed" => {
                self.publish_connection_event(AgentConnectionEvent::ThreadStatusChanged(
                    parse_thread_status_changed(message)?,
                ));
                Ok(())
            }
            // Account notifications are application-level: they never bind to
            // a thread or turn, and they never end an active turn.
            "account/rateLimits/updated" => self.handle_rate_limits_updated(connection, message),
            "account/updated" => self.handle_account_updated(connection, message),
            "account/login/completed" => self.handle_login_completed(connection, message),
            "warning" => {
                let thread_id = match message.pointer("/params/threadId") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(thread_id)) => Some(thread_id.clone()),
                    Some(_) => bail!("warning params.threadId 必须是字符串或 null"),
                };
                let Some(AgentEvent::Warning { message }) = parse_agent_notification(message)?
                else {
                    bail!("warning 未映射为 Agent warning");
                };
                self.publish_connection_event(AgentConnectionEvent::Warning { thread_id, message });
                Ok(())
            }
            "configWarning" => {
                let Some(AgentEvent::ConfigWarning(warning)) = parse_agent_notification(message)?
                else {
                    bail!("configWarning 未映射为 AgentConfigWarning");
                };
                self.publish_connection_event(AgentConnectionEvent::ConfigWarning(warning));
                Ok(())
            }
            "remoteControl/status/changed" => {
                validate_remote_control_status_changed(message)?;
                connection
                    .state
                    .lock()
                    .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?
                    .remote_control_status = message.get("params").cloned();
                Ok(())
            }
            method if TURN_SCOPED_SERVER_METHODS.contains(&method) => {
                let thread_id = message
                    .pointer("/params/threadId")
                    .and_then(Value::as_str)
                    .with_context(|| format!("{method} 消息缺少字符串 params.threadId"))?;
                let turn_id = turn_id_from_turn_message(message)?;
                let finished = connection
                    .state
                    .lock()
                    .map_err(|_| anyhow!("轮次注册表锁不可用"))?
                    .finished_turns
                    .contains(&super::connection::TurnKey {
                        thread_id: thread_id.to_owned(),
                        turn_id: turn_id.clone(),
                    });
                if finished {
                    return Ok(());
                }
                // A turn the server started by itself (goal continuation, queue
                // advance, queue/start) has no local owner until its own
                // turn/started adopts it.
                let turn = match connection.route(thread_id, &turn_id, message)? {
                    TurnRoute::Owned(turn) => turn,
                    // The pending turn/start has no id yet; the message waits
                    // for its response to tell whose it is.
                    TurnRoute::Buffered => return Ok(()),
                    TurnRoute::Unowned
                        if method == "turn/started"
                            && connection.claim_review_alias(thread_id, &turn_id)? =>
                    {
                        return Ok(());
                    }
                    TurnRoute::Unowned if method == "turn/started" => {
                        self.adopt_server_turn(connection, thread_id, &turn_id)?
                    }
                    TurnRoute::Unowned => {
                        bail!("收到未知 turn 的消息：threadId=`{thread_id}`，turnId=`{turn_id}`")
                    }
                };
                if let Some(outcome) = turn.ingest(message)? {
                    connection.finish_turn(&turn, Ok(outcome));
                }
                Ok(())
            }
            // Schema-known but unintegrated: accepted undecoded and recorded, so
            // an unprompted notice cannot fail the shared connection.
            method if is_unintegrated_server_notification(message) => {
                connection.record_unintegrated_notification(unintegrated_notification_diagnostic(
                    method, message,
                ));
                Ok(())
            }
            _ => ensure_server_method_is_defined(message),
        }
    }
    pub(super) fn handle_thread_started(
        &self,
        connection: &Connection,
        message: &Value,
    ) -> Result<()> {
        let thread_id = thread_started_id(message)?;
        let mut state = connection
            .state
            .lock()
            .map_err(|_| anyhow!("Codex connection state 锁已损坏"))?;
        if state.loaded_threads.contains(&thread_id)
            || state.permission_probe_threads.contains(&thread_id)
        {
            return Ok(());
        }
        if let Some(pending) = state.pending_thread_lifecycle.as_mut() {
            if let ThreadLifecycleKind::Resume(expected) = &pending.kind
                && expected != &thread_id
            {
                bail!("thread/resume `{expected}` 收到其他 thread 的 thread/started `{thread_id}`");
            }
            if let Some(observed) = &pending.observed_thread_id
                && observed != &thread_id
            {
                bail!(
                    "同一 thread lifecycle 收到不一致的 thread/started：`{observed}` 与 `{thread_id}`"
                );
            }
            pending.observed_thread_id = Some(thread_id);
            return Ok(());
        }
        bail!("收到未关联 lifecycle 的 thread/started `{thread_id}`")
    }
    pub(super) fn publish_connection_event(&self, event: AgentConnectionEvent) {
        if let Ok(mut hub) = self.connection_events.lock() {
            hub.publish(event);
        }
    }

    pub(super) fn publish_runtime(
        &self,
        generation: u64,
        observation: crate::agent::AgentRuntimeObservation,
    ) -> Result<()> {
        self.connection_events
            .lock()
            .map_err(|_| anyhow!("connection event hub 锁已损坏"))?
            .publish_runtime(crate::agent::AgentRuntimeEvent {
                generation,
                observation,
            })
    }
}
