//! Detection and import of configuration owned by other coding agents.
//! An import is closed by the server's own progress and completed
//! notifications, correlated by the import id returned from the request.

use std::sync::Arc;

use async_channel::Receiver;
use serde_json::Value;

use super::{CodexAppServerManager, connection::Connection};
use crate::agent::{
    AgentExternalAgentConfigError, AgentExternalAgentConfigErrorKind,
    AgentExternalAgentDetectRequest, AgentExternalAgentDetectResult,
    AgentExternalAgentHistoryRecordRequest, AgentExternalAgentImportHistories,
    AgentExternalAgentImportReceipt, AgentExternalAgentImportRequest,
};

fn connection_error(error: anyhow::Error) -> AgentExternalAgentConfigError {
    AgentExternalAgentConfigError {
        kind: AgentExternalAgentConfigErrorKind::Connection,
        message: format!("{error:#}"),
        data: None,
        outcome_unknown: false,
    }
}

fn protocol_error(error: anyhow::Error) -> AgentExternalAgentConfigError {
    AgentExternalAgentConfigError {
        kind: AgentExternalAgentConfigErrorKind::Protocol,
        message: format!("{error:#}"),
        data: None,
        outcome_unknown: false,
    }
}

fn decode_result<T>(
    method: &str,
    response: Value,
    decode: impl FnOnce(&Value) -> Result<T, anyhow::Error>,
) -> Result<T, AgentExternalAgentConfigError> {
    let result = response
        .get("result")
        .cloned()
        .ok_or_else(|| AgentExternalAgentConfigError {
            kind: AgentExternalAgentConfigErrorKind::Protocol,
            message: format!("{method} 响应缺少 result"),
            data: response.get("error").cloned(),
            outcome_unknown: false,
        })?;
    decode(&result).map_err(protocol_error)
}

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn detect_external_agent_config(
        &self,
        request: AgentExternalAgentDetectRequest,
    ) -> Receiver<Result<AgentExternalAgentDetectResult, AgentExternalAgentConfigError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(connection_error)
                .and_then(|connection| {
                    Self::detect_external_agent_config_on(&connection, &request)
                });
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn detect_external_agent_config_on(
        connection: &Arc<Connection>,
        request: &AgentExternalAgentDetectRequest,
    ) -> Result<AgentExternalAgentDetectResult, AgentExternalAgentConfigError> {
        let params = super::super::external_agent_config::detect_params(request);
        let response = connection
            .request("externalAgentConfig/detect", params)
            .map_err(connection_error)?;
        decode_result("externalAgentConfig/detect", response, |result| {
            super::super::external_agent_config::decode_detect(
                connection.generation,
                request,
                result,
            )
        })
    }

    pub(in crate::agent::codex) fn import_external_agent_config(
        &self,
        request: AgentExternalAgentImportRequest,
    ) -> Receiver<Result<AgentExternalAgentImportReceipt, AgentExternalAgentConfigError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(connection_error)
                .and_then(|connection| {
                    Self::import_external_agent_config_on(&connection, &request)
                });
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn import_external_agent_config_on(
        connection: &Arc<Connection>,
        request: &AgentExternalAgentImportRequest,
    ) -> Result<AgentExternalAgentImportReceipt, AgentExternalAgentConfigError> {
        let params = super::super::external_agent_config::import_params(request);
        let response = connection
            .request("externalAgentConfig/import", params)
            .map_err(connection_error)?;
        decode_result("externalAgentConfig/import", response, |result| {
            super::super::external_agent_config::decode_import_receipt(result)
        })
    }

    pub(in crate::agent::codex) fn read_external_agent_import_histories(
        &self,
    ) -> Receiver<Result<AgentExternalAgentImportHistories, AgentExternalAgentConfigError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(connection_error)
                .and_then(|connection| Self::read_external_agent_import_histories_on(&connection));
            let _ = sender.send_blocking(result);
        });
        receiver
    }

    pub(super) fn read_external_agent_import_histories_on(
        connection: &Arc<Connection>,
    ) -> Result<AgentExternalAgentImportHistories, AgentExternalAgentConfigError> {
        let response = connection
            .request(
                "externalAgentConfig/import/readHistories",
                Value::Object(Default::default()),
            )
            .map_err(connection_error)?;
        decode_result(
            "externalAgentConfig/import/readHistories",
            response,
            |result| {
                super::super::external_agent_config::decode_histories(connection.generation, result)
            },
        )
    }

    pub(in crate::agent::codex) fn record_external_agent_import_history(
        &self,
        request: AgentExternalAgentHistoryRecordRequest,
    ) -> Receiver<Result<AgentExternalAgentImportReceipt, AgentExternalAgentConfigError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            let result = manager
                .inner
                .ensure_connection()
                .map_err(connection_error)
                .and_then(|connection| {
                    let params =
                        super::super::external_agent_config::history_record_params(&request);
                    let response = connection
                        .request("externalAgentConfig/import/recordHistory", params)
                        .map_err(connection_error)?;
                    decode_result(
                        "externalAgentConfig/import/recordHistory",
                        response,
                        |result| {
                            super::super::external_agent_config::decode_history_record_receipt(
                                result,
                            )
                        },
                    )
                });
            let _ = sender.send_blocking(result);
        });
        receiver
    }
}
