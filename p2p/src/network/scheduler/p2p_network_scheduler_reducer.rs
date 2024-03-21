use std::collections::BTreeMap;

use openmina_core::error;

use crate::{
    channels::{ChannelId, P2pChannelsState},
    P2pPeerState, P2pPeerStatus, P2pPeerStatusReady, PeerId,
};

use super::{super::*, p2p_network_scheduler_state::P2pNetworkConnectionState, *};

impl P2pNetworkSchedulerState {
    pub fn reducer(
        &mut self,
        peers: &mut BTreeMap<PeerId, P2pPeerState>,
        action: redux::ActionWithMeta<&P2pNetworkSchedulerAction>,
    ) {
        let (action, meta) = action.split();
        match action {
            P2pNetworkSchedulerAction::InterfaceDetected { ip, .. } => {
                self.interfaces.insert(*ip);
            }
            P2pNetworkSchedulerAction::InterfaceExpired { ip, .. } => {
                self.interfaces.remove(ip);
            }
            P2pNetworkSchedulerAction::IncomingConnectionIsReady { .. } => {}
            P2pNetworkSchedulerAction::IncomingDidAccept { addr, .. } => {
                let Some(addr) = addr else {
                    return;
                };

                self.connections.insert(
                    *addr,
                    P2pNetworkConnectionState {
                        incoming: true,
                        pnet: P2pNetworkPnetState::new(self.pnet_key),
                        select_auth: P2pNetworkSelectState::default(),
                        auth: None,
                        select_mux: P2pNetworkSelectState::default(),
                        mux: None,
                        streams: BTreeMap::default(),
                    },
                );
            }
            P2pNetworkSchedulerAction::OutgoingDidConnect { addr, .. } => {
                self.connections.insert(
                    *addr,
                    P2pNetworkConnectionState {
                        incoming: false,
                        pnet: P2pNetworkPnetState::new(self.pnet_key),
                        select_auth: P2pNetworkSelectState::initiator_auth(token::AuthKind::Noise),
                        auth: None,
                        select_mux: P2pNetworkSelectState::initiator_mux(
                            token::MuxKind::Yamux1_0_0,
                        ),
                        mux: None,
                        streams: BTreeMap::default(),
                    },
                );
            }
            P2pNetworkSchedulerAction::IncomingDataIsReady { .. } => {}
            P2pNetworkSchedulerAction::IncomingDataDidReceive { result, addr, .. } => {
                if result.is_err() {
                    self.connections.remove(addr);
                }
            }
            P2pNetworkSchedulerAction::SelectDone {
                addr,
                kind,
                protocol,
                incoming,
                ..
            } => {
                let Some(connection) = self.connections.get_mut(addr) else {
                    return;
                };
                match kind {
                    SelectKind::Multiplexing(peer_id) => {
                        let enabled_channels = Some(ChannelId::Rpc).into_iter().collect();
                        let state = P2pPeerState {
                            is_libp2p: true,
                            dial_opts: None,
                            status: P2pPeerStatus::Ready(P2pPeerStatusReady {
                                is_incoming: *incoming,
                                connected_since: meta.time(),
                                channels: P2pChannelsState::new(&enabled_channels),
                                best_tip: None,
                            }),
                        };
                        peers.insert(*peer_id, state);
                    }
                    _ => {}
                }
                match protocol {
                    Some(token::Protocol::Auth(token::AuthKind::Noise)) => {
                        connection.auth = Some(P2pNetworkAuthState::Noise(P2pNetworkNoiseState {
                            handshake_optimized: true,
                            ..Default::default()
                        }));
                    }
                    Some(token::Protocol::Mux(
                        token::MuxKind::Yamux1_0_0 | token::MuxKind::YamuxNoNewLine1_0_0,
                    )) => {
                        connection.mux =
                            Some(P2pNetworkConnectionMuxState::Yamux(P2pNetworkYamuxState {
                                init: true,
                                ..Default::default()
                            }));
                    }
                    Some(token::Protocol::Stream(stream_kind)) => {
                        let SelectKind::Stream(peer_id, stream_id) = kind else {
                            error!(meta.time(); "incorrect stream kind {kind:?} for protocol stream: {stream_kind:?}");
                            return;
                        };
                        match stream_kind {
                            token::StreamKind::Rpc(_) => {
                                if *incoming {
                                    self.rpc_incoming_streams
                                        .entry(*peer_id)
                                        .or_default()
                                        .insert(
                                            *stream_id,
                                            P2pNetworkRpcState::new(*addr, *stream_id),
                                        );
                                } else {
                                    self.rpc_outgoing_streams
                                        .entry(*peer_id)
                                        .or_default()
                                        .insert(
                                            *stream_id,
                                            P2pNetworkRpcState::new(*addr, *stream_id),
                                        );
                                }
                            }
                            token::StreamKind::Broadcast(_) => unimplemented!(),
                            token::StreamKind::Discovery(_) => {}
                        }
                    }
                    None => {}
                }
            }
            P2pNetworkSchedulerAction::SelectError { addr, kind, .. } => {
                if let Some(stream_id) = &kind.stream_id() {
                    if let Some(connection) = self.connections.get_mut(addr) {
                        connection.streams.remove(stream_id);
                    }
                } else {
                    self.connections.remove(addr);
                }
            }
            P2pNetworkSchedulerAction::YamuxDidInit { addr, .. } => {
                if let Some(cn) = self.connections.get_mut(addr) {
                    if let Some(P2pNetworkConnectionMuxState::Yamux(yamux)) = &mut cn.mux {
                        yamux.init = true;
                    }
                }
            }
        }
    }
}
