use std::sync::Arc;

use openmina_core::{block::BlockWithHash, fuzz_maybe, fuzzed_maybe};

use crate::{
    channels::{snark::P2pChannelsSnarkAction, transaction::P2pChannelsTransactionAction},
    peer::P2pPeerAction,
    P2pCryptoService, P2pNetworkYamuxAction,
};

use super::{pb, P2pNetworkPubsubAction, TOPIC};

fn message_is_empty(msg: &pb::Rpc) -> bool {
    msg.subscriptions.is_empty() && msg.publish.is_empty() && msg.control.is_none()
}

impl P2pNetworkPubsubAction {
    pub fn effects<Store, S>(self, _meta: &redux::ActionMeta, store: &mut Store)
    where
        Store: crate::P2pStore<S>,
        Store::Service: P2pCryptoService,
    {
        let state = &store.state().network.scheduler.broadcast_state;
        let config = &store.state().config;

        match self {
            P2pNetworkPubsubAction::NewStream {
                peer_id, incoming, ..
            } => {
                if !incoming {
                    let subscrption = {
                        let msg = pb::Rpc {
                            subscriptions: vec![pb::rpc::SubOpts {
                                subscribe: Some(true),
                                topic_id: Some(TOPIC.to_owned()),
                            }],
                            publish: vec![],
                            control: None,
                        };
                        Some(P2pNetworkPubsubAction::OutgoingMessage { msg, peer_id })
                    };
                    let graft = if state.clients.len() < config.meshsub.outbound_degree_desired {
                        Some(P2pNetworkPubsubAction::Graft {
                            peer_id,
                            topic_id: TOPIC.to_owned(),
                        })
                    } else {
                        None
                    };
                    for action in subscrption.into_iter().chain(graft) {
                        store.dispatch(action);
                    }
                }
            }
            P2pNetworkPubsubAction::Graft { peer_id, topic_id } => {
                let msg = pb::Rpc {
                    subscriptions: vec![],
                    publish: vec![],
                    control: Some(pb::ControlMessage {
                        ihave: vec![],
                        iwant: vec![],
                        graft: vec![pb::ControlGraft {
                            topic_id: Some(dbg!(topic_id.clone())),
                        }],
                        prune: vec![],
                    }),
                };

                store.dispatch(P2pNetworkPubsubAction::OutgoingMessage { msg, peer_id });
            }
            P2pNetworkPubsubAction::Broadcast { message } => {
                let mut buffer = vec![0; 8];
                binprot::BinProtWrite::binprot_write(&message, &mut buffer).expect("msg");
                let len = buffer.len() - 8;
                buffer[..8].clone_from_slice(&(len as u64).to_le_bytes());

                store.dispatch(P2pNetworkPubsubAction::Sign {
                    seqno: state.seq + config.meshsub.initial_time.as_nanos() as u64,
                    author: config.identity_pub_key.peer_id(),
                    data: buffer.into(),
                    topic: TOPIC.to_owned(),
                });
            }
            P2pNetworkPubsubAction::Sign { .. } => {
                if let Some(to_sign) = state.to_sign.front() {
                    let mut publication = vec![];
                    prost::Message::encode(to_sign, &mut publication).unwrap();
                    let signature = store.service().sign_publication(&publication).into();
                    store.dispatch(P2pNetworkPubsubAction::BroadcastSigned { signature });
                }
            }
            P2pNetworkPubsubAction::BroadcastSigned { .. } => broadcast(store),
            P2pNetworkPubsubAction::IncomingData { peer_id, .. } => {
                let incoming_block = state.incoming_block.as_ref().cloned();
                let incoming_transactions = state.incoming_transactions.clone();
                let incoming_snarks = state.incoming_snarks.clone();
                let topics = state.topics.clone();
                let could_accept = state.clients.len() < config.meshsub.outbound_degree_high;

                for (topic_id, map) in topics {
                    if let Some(mesh_state) = map.get(&peer_id) {
                        let _ = (could_accept, topic_id, mesh_state);
                        // TODO: prune
                    }
                }

                broadcast(store);
                if let Some((_, block)) = incoming_block {
                    let best_tip = BlockWithHash::new(Arc::new(block));
                    store.dispatch(P2pPeerAction::BestTipUpdate { peer_id, best_tip });
                }
                for (transaction, nonce) in incoming_transactions {
                    store.dispatch(P2pChannelsTransactionAction::Libp2pReceived {
                        peer_id,
                        transaction: Box::new(transaction),
                        nonce,
                    });
                }
                for (snark, nonce) in incoming_snarks {
                    store.dispatch(P2pChannelsSnarkAction::Libp2pReceived {
                        peer_id,
                        snark: Box::new(snark),
                        nonce,
                    });
                }
            }
            P2pNetworkPubsubAction::OutgoingMessage { msg, peer_id } => {
                if !message_is_empty(&msg) {
                    let mut data = vec![];
                    if prost::Message::encode_length_delimited(&msg, &mut data).is_ok() {
                        store.dispatch(P2pNetworkPubsubAction::OutgoingData {
                            data: data.clone().into(),
                            peer_id,
                        });
                    }
                }
            }
            P2pNetworkPubsubAction::OutgoingData { mut data, peer_id } => {
                let Some(state) = store
                    .state()
                    .network
                    .scheduler
                    .broadcast_state
                    .clients
                    .get(&peer_id)
                else {
                    return;
                };
                fuzz_maybe!(&mut data, crate::fuzzer::mutate_pubsub);
                let flags = fuzzed_maybe!(Default::default(), crate::fuzzer::mutate_yamux_flags);

                if let Some(stream_id) = state.outgoing_stream_id.as_ref().copied() {
                    store.dispatch(P2pNetworkYamuxAction::OutgoingData {
                        addr: state.addr,
                        stream_id,
                        data,
                        flags,
                    });
                }
            }
        }
    }
}

fn broadcast<Store, S>(store: &mut Store)
where
    Store: crate::P2pStore<S>,
{
    let state = &store.state().network.scheduler.broadcast_state;
    let broadcast = state
        .clients
        .iter()
        .filter(|(_, state)| !message_is_empty(&state.message))
        .map(|(peer_id, state)| P2pNetworkPubsubAction::OutgoingMessage {
            msg: state.message.clone(),
            peer_id: *peer_id,
        })
        .collect::<Vec<_>>();
    for action in broadcast {
        store.dispatch(action);
    }
}
