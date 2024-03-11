use std::sync::Arc;

use redux::ActionMeta;

use crate::block_producer::to_epoch_and_slot;
use crate::Service;
use crate::Store;

use super::BlockProducerVrfEvaluatorAction;
use super::SlotPositionInEpoch;

impl BlockProducerVrfEvaluatorAction {
    pub fn effects<S: Service>(self, _: &ActionMeta, store: &mut Store<S>) {
        match self {
            BlockProducerVrfEvaluatorAction::EvaluateSlot { vrf_input } => {
                store.service.evaluate(vrf_input);
            }
            BlockProducerVrfEvaluatorAction::ProcessSlotEvaluationSuccess {
                vrf_output, ..
            } => {
                if let Some(vrf_evaluator_state) = store.state().block_producer.vrf_evaluator() {
                    if let Some(pending_evaluation) = vrf_evaluator_state.current_evaluation() {
                        store.dispatch(BlockProducerVrfEvaluatorAction::CheckEpochBounds {
                            epoch_number: pending_evaluation.epoch_number,
                            latest_evaluated_global_slot: vrf_output.global_slot(),
                        });
                    }
                }
            }
            BlockProducerVrfEvaluatorAction::CheckEpochBounds {
                latest_evaluated_global_slot,
                epoch_number,
            } => {
                if let Some(epoch_bound) = store
                    .state()
                    .block_producer
                    .vrf_evaluator()
                    .and_then(|s| s.get_epoch_bound_from_check())
                {
                    match epoch_bound {
                        SlotPositionInEpoch::Beginning | SlotPositionInEpoch::Within => {
                            store.dispatch(
                                BlockProducerVrfEvaluatorAction::ContinueEpochEvaluation {
                                    latest_evaluated_global_slot,
                                    epoch_number,
                                },
                            );
                        }
                        SlotPositionInEpoch::End => {
                            store.dispatch(
                                BlockProducerVrfEvaluatorAction::FinishEpochEvaluation {
                                    latest_evaluated_global_slot,
                                    epoch_number,
                                },
                            );
                        }
                    }
                }
            }
            BlockProducerVrfEvaluatorAction::InitializeEvaluator { best_tip } => {
                // Note: pure function, but needs access to other parts of the state
                if store.state().block_producer.vrf_evaluator().is_some() {
                    if best_tip.consensus_state().epoch_count.as_u32() == 0 {
                        store.dispatch(
                            BlockProducerVrfEvaluatorAction::FinalizeEvaluatorInitialization {
                                previous_epoch_and_height: None,
                            },
                        );
                    } else {
                        let k = best_tip.constants().k.as_u32();
                        let (epoch, slot) = to_epoch_and_slot(
                            &best_tip.consensus_state().curr_global_slot_since_hard_fork,
                        );
                        let last_height = if slot < k {
                            store
                                .state()
                                .transition_frontier
                                .best_chain
                                .iter()
                                .rev()
                                .find(|b| b.consensus_state().epoch_count.as_u32() == epoch - 1)
                                .unwrap()
                                .height()
                        } else {
                            store
                                .state()
                                .transition_frontier
                                .sync
                                .root_block()
                                .unwrap()
                                .height()
                        };
                        store.dispatch(
                            BlockProducerVrfEvaluatorAction::FinalizeEvaluatorInitialization {
                                previous_epoch_and_height: Some((epoch - 1, last_height)),
                            },
                        );
                    }
                }
            }
            BlockProducerVrfEvaluatorAction::FinalizeEvaluatorInitialization { .. } => {}
            BlockProducerVrfEvaluatorAction::CheckEpochEvaluability {
                current_best_tip_height,
                current_best_tip_global_slot,
                current_epoch_number,
                current_best_tip_slot,
                transition_frontier_size,
                next_epoch_first_slot,
                ..
            } => {
                let vrf_evaluator_state = store.state().block_producer.vrf_evaluator_with_config();

                if let Some((vrf_evaluator_state, config)) = vrf_evaluator_state {
                    let last_epoch_block_height: Option<u32> =
                        vrf_evaluator_state.last_height(current_epoch_number - 1);
                    if let Some(epoch_data) = vrf_evaluator_state.epoch_context().get_epoch_data() {
                        store.dispatch(
                            BlockProducerVrfEvaluatorAction::InitializeEpochEvaluation {
                                staking_epoch_data: epoch_data,
                                producer: config.pub_key.clone().into(),
                                current_best_tip_height,
                                current_best_tip_global_slot,
                                current_epoch_number,
                                current_best_tip_slot,
                                transition_frontier_size,
                                next_epoch_first_slot,
                            },
                        );
                    } else {
                        // If None is returned, than we are waiting for evaluation
                        store.dispatch(BlockProducerVrfEvaluatorAction::WaitForNextEvaluation {
                            current_epoch_number,
                            current_best_tip_height,
                            current_best_tip_global_slot,
                            current_best_tip_slot,
                            last_epoch_block_height,
                            transition_frontier_size,
                        });
                    }
                }
            }
            BlockProducerVrfEvaluatorAction::InitializeEpochEvaluation {
                staking_epoch_data,
                producer,
                current_epoch_number,
                current_best_tip_height,
                current_best_tip_global_slot,
                current_best_tip_slot,
                transition_frontier_size,
                next_epoch_first_slot,
            } => {
                store.dispatch(
                    BlockProducerVrfEvaluatorAction::BeginDelegatorTableConstruction {
                        staking_epoch_data,
                        producer,
                        current_best_tip_height,
                        current_best_tip_global_slot,
                        current_best_tip_slot,
                        current_epoch_number,
                        transition_frontier_size,
                        next_epoch_first_slot,
                    },
                );
            }
            BlockProducerVrfEvaluatorAction::BeginDelegatorTableConstruction {
                staking_epoch_data,
                producer,
                current_epoch_number,
                current_best_tip_height,
                current_best_tip_slot,
                current_best_tip_global_slot,
                next_epoch_first_slot,
                transition_frontier_size,
            } => {
                let delegator_table = store.service().get_producer_and_delegates(
                    staking_epoch_data.ledger.clone(),
                    producer.clone(),
                );
                let mut epoch_data = staking_epoch_data.clone();
                epoch_data.delegator_table = Arc::new(delegator_table);

                store.dispatch(
                    BlockProducerVrfEvaluatorAction::FinalizeDelegatorTableConstruction {
                        staking_epoch_data: epoch_data,
                        producer,
                        current_epoch_number,
                        current_best_tip_height,
                        current_best_tip_slot,
                        current_best_tip_global_slot,
                        next_epoch_first_slot,
                        transition_frontier_size,
                    },
                );
            }
            BlockProducerVrfEvaluatorAction::FinalizeDelegatorTableConstruction {
                current_best_tip_height,
                current_best_tip_global_slot,
                current_best_tip_slot,
                current_epoch_number,
                staking_epoch_data,
                next_epoch_first_slot,
                ..
            } => {
                let current_global_slot =
                    if let Some(current_global_slot) = store.state().cur_global_slot() {
                        current_global_slot
                    } else {
                        // error here!
                        return;
                    };
                store.dispatch(BlockProducerVrfEvaluatorAction::SelectInitialSlot {
                    current_global_slot,
                    current_best_tip_height,
                    current_best_tip_global_slot,
                    current_best_tip_slot,
                    current_epoch_number,
                    staking_epoch_data,
                    next_epoch_first_slot,
                });
            }
            BlockProducerVrfEvaluatorAction::BeginEpochEvaluation {
                latest_evaluated_global_slot,
                current_epoch_number,
                ..
            } => {
                if store.state().block_producer.vrf_evaluator().is_some() {
                    store.dispatch(BlockProducerVrfEvaluatorAction::ContinueEpochEvaluation {
                        latest_evaluated_global_slot,
                        epoch_number: current_epoch_number,
                    });
                }
            }
            BlockProducerVrfEvaluatorAction::RecordLastBlockHeightInEpoch { .. } => {}
            BlockProducerVrfEvaluatorAction::ContinueEpochEvaluation { .. } => {
                if let Some(vrf_evaluator_state) = store.state().block_producer.vrf_evaluator() {
                    if let Some(vrf_input) = vrf_evaluator_state.construct_vrf_input() {
                        store.dispatch(BlockProducerVrfEvaluatorAction::EvaluateSlot { vrf_input });
                    }
                }
            }
            BlockProducerVrfEvaluatorAction::FinishEpochEvaluation { .. } => {}
            BlockProducerVrfEvaluatorAction::WaitForNextEvaluation { .. } => {}
            BlockProducerVrfEvaluatorAction::SelectInitialSlot {
                current_epoch_number,
                current_best_tip_height,
                current_best_tip_global_slot,
                current_best_tip_slot,
                staking_epoch_data,
                ..
            } => {
                if let Some(initial_slot) = store
                    .state()
                    .block_producer
                    .vrf_evaluator()
                    .and_then(|v| v.initial_slot())
                {
                    store.dispatch(BlockProducerVrfEvaluatorAction::BeginEpochEvaluation {
                        current_epoch_number,
                        current_best_tip_height,
                        current_best_tip_global_slot,
                        current_best_tip_slot,
                        staking_epoch_data,
                        latest_evaluated_global_slot: initial_slot,
                    });
                }
            }
        }
    }
}
