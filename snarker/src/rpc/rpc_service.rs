use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::p2p::webrtc;
use crate::State;

use super::{ActionStatsResponse, RpcId};

#[derive(Error, Serialize, Deserialize, Debug, Clone)]
pub enum RespondError {
    #[error("unknown rpc id")]
    UnknownRpcId,
    #[error("unexpected response type")]
    UnexpectedResponseType,
}

#[derive(Error, Serialize, Deserialize, Debug, Clone)]
pub enum WatchedAccountsGetError {
    #[error("requested account isn't being watched")]
    NotWatching,
    #[error("not ready to respond, try again later")]
    NotReady,
}

pub trait RpcService: redux::Service {
    fn respond_state_get(&mut self, rpc_id: RpcId, response: &State) -> Result<(), RespondError>;
    fn respond_action_stats_get(
        &mut self,
        rpc_id: RpcId,
        response: Option<ActionStatsResponse>,
    ) -> Result<(), RespondError>;
    fn respond_p2p_connection_outgoing(
        &mut self,
        rpc_id: RpcId,
        response: Result<(), String>,
    ) -> Result<(), RespondError>;
    fn respond_p2p_connection_incoming_answer(
        &mut self,
        rpc_id: RpcId,
        response: Result<webrtc::Answer, String>,
    ) -> Result<(), RespondError>;
    fn respond_p2p_connection_incoming(
        &mut self,
        rpc_id: RpcId,
        response: Result<(), String>,
    ) -> Result<(), RespondError>;
}
