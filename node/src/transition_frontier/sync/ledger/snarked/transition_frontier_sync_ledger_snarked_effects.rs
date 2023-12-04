use mina_p2p_messages::v2::MinaLedgerSyncLedgerQueryStableV1;
use p2p::channels::rpc::{P2pChannelsRpcRequestSendAction, P2pRpcRequest};
use p2p::PeerId;
use redux::ActionMeta;

use crate::ledger::{LedgerAddress, LEDGER_DEPTH};
use crate::transition_frontier::sync::TransitionFrontierSyncState;
use crate::Store;

use super::{
    PeerLedgerQueryResponse, TransitionFrontierSyncLedgerSnarkedChildAccountsReceivedAction,
    TransitionFrontierSyncLedgerSnarkedChildHashesReceivedAction,
    TransitionFrontierSyncLedgerSnarkedPeerQueryErrorAction,
    TransitionFrontierSyncLedgerSnarkedPeerQueryInitAction,
    TransitionFrontierSyncLedgerSnarkedPeerQueryPendingAction,
    TransitionFrontierSyncLedgerSnarkedPeerQueryRetryAction,
    TransitionFrontierSyncLedgerSnarkedPeerQuerySuccessAction,
    TransitionFrontierSyncLedgerSnarkedPeersQueryAction,
    TransitionFrontierSyncLedgerSnarkedPendingAction, TransitionFrontierSyncLedgerSnarkedService,
    TransitionFrontierSyncLedgerSnarkedSuccessAction,
};

fn query_peer_init<S: redux::Service>(
    store: &mut Store<S>,
    peer_id: PeerId,
    address: LedgerAddress,
) {
    let Some((ledger_hash, rpc_id)) = None.or_else(|| {
        let state = store.state();
        let ledger = state.transition_frontier.sync.ledger()?;
        // TODO(tizoc): depending on state the hash is different
        let ledger_hash = match store.state().transition_frontier.sync {
            TransitionFrontierSyncState::StakingLedgerPending { .. } => {
                &ledger
                    .block()
                    .header()
                    .protocol_state
                    .body
                    .consensus_state
                    .staking_epoch_data
                    .ledger
                    .hash
            }
            TransitionFrontierSyncState::NextEpochLedgerPending { .. } => {
                &ledger
                    .block()
                    .header()
                    .protocol_state
                    .body
                    .consensus_state
                    .next_epoch_data
                    .ledger
                    .hash
            }
            TransitionFrontierSyncState::RootLedgerPending { .. } => ledger.snarked_ledger_hash(),
            _ => {
                println!("@@@@@ unexpected state when handling query_peer_init");
                return None;
            }
        };

        let p = store.state().p2p.get_ready_peer(&peer_id)?;
        let rpc_id = p.channels.rpc.next_local_rpc_id();

        Some((ledger_hash.clone(), rpc_id))
    }) else {
        return;
    };

    let query = if address.length() >= LEDGER_DEPTH - 1 {
        MinaLedgerSyncLedgerQueryStableV1::WhatContents(address.clone().into())
    } else {
        MinaLedgerSyncLedgerQueryStableV1::WhatChildHashes(address.clone().into())
    };

    if store.dispatch(P2pChannelsRpcRequestSendAction {
        peer_id,
        id: rpc_id,
        request: P2pRpcRequest::LedgerQuery(ledger_hash, query),
    }) {
        store.dispatch(TransitionFrontierSyncLedgerSnarkedPeerQueryPendingAction {
            address,
            peer_id,
            rpc_id,
        });
    }
}

impl TransitionFrontierSyncLedgerSnarkedPendingAction {
    pub fn effects<S: redux::Service>(self, _: &ActionMeta, store: &mut Store<S>) {
        store.dispatch(TransitionFrontierSyncLedgerSnarkedPeersQueryAction {});
    }
}

impl TransitionFrontierSyncLedgerSnarkedPeersQueryAction {
    pub fn effects<S: redux::Service>(self, _: &ActionMeta, store: &mut Store<S>) {
        // TODO(binier): make sure they have the ledger we want to query.
        let mut peer_ids = store
            .state()
            .p2p
            .ready_peers_iter()
            .filter(|(_, p)| p.channels.rpc.can_send_request())
            .map(|(id, p)| (*id, p.connected_since))
            .collect::<Vec<_>>();
        peer_ids.sort_by(|(_, t1), (_, t2)| t2.cmp(t1));

        let mut retry_addresses = store
            .state()
            .transition_frontier
            .sync
            .ledger()
            .and_then(|s| s.snarked())
            .map_or(vec![], |s| s.sync_retry_iter().collect());
        retry_addresses.reverse();

        // TODO(tizoc): revise and document this logic
        for (peer_id, _) in peer_ids {
            if let Some(address) = retry_addresses.last() {
                if store.dispatch(TransitionFrontierSyncLedgerSnarkedPeerQueryRetryAction {
                    peer_id,
                    address: address.clone(),
                }) {
                    retry_addresses.pop();
                    continue;
                }
            }

            // TODO(tizoc): address should change depending on synchronization target
            let address = store
                .state()
                .transition_frontier
                .sync
                .ledger()
                .and_then(|s| s.snarked())
                .and_then(|s| s.sync_next());
            match address {
                Some(address) => {
                    store.dispatch(TransitionFrontierSyncLedgerSnarkedPeerQueryInitAction {
                        peer_id,
                        address,
                    });
                }
                None if retry_addresses.is_empty() => break,
                None => {}
            }
        }
    }
}

impl TransitionFrontierSyncLedgerSnarkedPeerQueryInitAction {
    pub fn effects<S: redux::Service>(self, _: &ActionMeta, store: &mut Store<S>) {
        query_peer_init(store, self.peer_id, self.address);
    }
}

impl TransitionFrontierSyncLedgerSnarkedPeerQueryRetryAction {
    pub fn effects<S: redux::Service>(self, _: &ActionMeta, store: &mut Store<S>) {
        query_peer_init(store, self.peer_id, self.address);
    }
}

impl TransitionFrontierSyncLedgerSnarkedPeerQueryErrorAction {
    pub fn effects<S: redux::Service>(self, _: &ActionMeta, store: &mut Store<S>) {
        store.dispatch(TransitionFrontierSyncLedgerSnarkedPeersQueryAction {});
    }
}

impl TransitionFrontierSyncLedgerSnarkedPeerQuerySuccessAction {
    pub fn effects<S: redux::Service>(self, _: &ActionMeta, store: &mut Store<S>) {
        let ledger = store.state().transition_frontier.sync.ledger();
        let Some(address) = ledger
            .and_then(|s| s.snarked()?.peer_query_get(&self.peer_id, self.rpc_id))
            .map(|(addr, _)| addr.clone())
        else {
            return;
        };

        match self.response {
            PeerLedgerQueryResponse::ChildHashes(left, right) => {
                store.dispatch(
                    TransitionFrontierSyncLedgerSnarkedChildHashesReceivedAction {
                        address,
                        hashes: (left, right),
                        sender: self.peer_id,
                    },
                );
            }
            PeerLedgerQueryResponse::ChildAccounts(accounts) => {
                store.dispatch(
                    TransitionFrontierSyncLedgerSnarkedChildAccountsReceivedAction {
                        address,
                        accounts,
                        sender: self.peer_id,
                    },
                );
            }
        }
    }
}

impl TransitionFrontierSyncLedgerSnarkedChildHashesReceivedAction {
    pub fn effects<S>(self, _: &ActionMeta, store: &mut Store<S>)
    where
        S: TransitionFrontierSyncLedgerSnarkedService,
    {
        let Some(block) = store.state().transition_frontier.sync.ledger() else {
            return;
        };
        let ledger_hash = match store.state().transition_frontier.sync {
            TransitionFrontierSyncState::StakingLedgerPending { .. } => block
                .block()
                .header()
                .protocol_state
                .body
                .consensus_state
                .staking_epoch_data
                .ledger
                .hash
                .clone(),
            TransitionFrontierSyncState::NextEpochLedgerPending { .. } => block
                .block()
                .header()
                .protocol_state
                .body
                .consensus_state
                .next_epoch_data
                .ledger
                .hash
                .clone(),
            TransitionFrontierSyncState::RootLedgerPending { .. } => {
                block.snarked_ledger_hash().clone()
            }
            _ => {
                println!("@@@@@ unexpected state when handling TransitionFrontierSyncLedgerSnarkedChildHashesReceivedAction");
                return;
            }
        };
        //// TODO(tizoc): either this block must match the end of the epochs or depending on the state
        //// we have to use a different block
        //// Also, what happens if the ledger changes during the request? this will not be correct
        //let snarked_ledger_hash = block.snarked_ledger_hash().clone();
        store
            .service
            .hashes_set(ledger_hash, &self.address, self.hashes)
            .unwrap();

        if !store.dispatch(TransitionFrontierSyncLedgerSnarkedPeersQueryAction {}) {
            store.dispatch(TransitionFrontierSyncLedgerSnarkedSuccessAction {});
        }
    }
}

impl TransitionFrontierSyncLedgerSnarkedChildAccountsReceivedAction {
    pub fn effects<S>(self, _: &ActionMeta, store: &mut Store<S>)
    where
        S: TransitionFrontierSyncLedgerSnarkedService,
    {
        let Some(block) = store.state().transition_frontier.sync.ledger() else {
            return;
        };
        let snarked_ledger_hash = block.snarked_ledger_hash().clone();
        store
            .service
            .accounts_set(snarked_ledger_hash, &self.address, self.accounts)
            .unwrap();

        if !store.dispatch(TransitionFrontierSyncLedgerSnarkedPeersQueryAction {}) {
            store.dispatch(TransitionFrontierSyncLedgerSnarkedSuccessAction {});
        }
    }
}
