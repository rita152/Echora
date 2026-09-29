//! `thread/searchOccurrences`: persisted threads are searched without being
//! loaded; the server answers `-32601` for threads it cannot search (an
//! ephemeral side chat), which the caller may answer with local text.

use async_channel::Receiver;

use super::CodexAppServerManager;
use crate::agent::{
    AgentThreadOccurrencePage, AgentThreadOccurrenceRequest, AgentThreadSearchError,
};

impl CodexAppServerManager {
    pub(in crate::agent::codex) fn search_thread_occurrences(
        &self,
        request: AgentThreadOccurrenceRequest,
    ) -> Receiver<Result<AgentThreadOccurrencePage, AgentThreadSearchError>> {
        let (sender, receiver) = async_channel::bounded(1);
        let manager = self.clone();
        std::thread::spawn(move || {
            use super::super::search::{SEARCH_OCCURRENCES_METHOD, params, parse_page};
            let result = (|| {
                let connection = manager
                    .inner
                    .ensure_connection()
                    .map_err(|error| AgentThreadSearchError::Failed(format!("{error:#}")))?;
                let response = connection
                    .request(SEARCH_OCCURRENCES_METHOD, params(&request))
                    .map_err(|error| {
                        let message = format!("{error:#}");
                        // The connection formats the JSON-RPC error object into
                        // the message; its code is the only signal kept.
                        if message.contains("\"code\":-32601") {
                            AgentThreadSearchError::Unsupported(message)
                        } else {
                            AgentThreadSearchError::Failed(message)
                        }
                    })?;
                let (occurrences, next_cursor) = parse_page(&response)
                    .map_err(|error| AgentThreadSearchError::Failed(format!("{error:#}")))?;
                Ok(AgentThreadOccurrencePage {
                    generation: connection.generation,
                    occurrences,
                    next_cursor,
                })
            })();
            let _ = sender.send_blocking(result);
        });
        receiver
    }
}
