import { FeatureAction } from '@openmina/shared';
import { BenchmarksWallet } from '@shared/types/benchmarks/wallets/benchmarks-wallet.type';
import { BenchmarksWalletTransaction } from '@shared/types/benchmarks/wallets/benchmarks-wallet-transaction.type';
import { MempoolTransaction } from '@shared/types/mempool/mempool-transaction.type';

enum BenchmarksWalletsActionTypes {
  BENCHMARKS_WALLETS_CLOSE = 'BENCHMARKS_WALLETS_CLOSE',
  BENCHMARKS_WALLETS_GET_WALLETS = 'BENCHMARKS_WALLETS_GET_WALLETS',
  BENCHMARKS_WALLETS_GET_WALLETS_SUCCESS = 'BENCHMARKS_WALLETS_GET_WALLETS_SUCCESS',
  BENCHMARKS_WALLETS_UPDATE_WALLETS_SUCCESS = 'BENCHMARKS_WALLETS_UPDATE_WALLETS_SUCCESS',
  BENCHMARKS_WALLETS_CHANGE_TRANSACTION_BATCH = 'BENCHMARKS_WALLETS_CHANGE_TRANSACTION_BATCH',
  BENCHMARKS_WALLETS_SEND_TXS = 'BENCHMARKS_WALLETS_SEND_TXS',
  BENCHMARKS_WALLETS_SEND_TX_SYNCED = 'BENCHMARKS_WALLETS_SEND_TX_SYNCED',
  BENCHMARKS_WALLETS_SEND_TX_SUCCESS = 'BENCHMARKS_WALLETS_SEND_TX_SUCCESS',
  BENCHMARKS_WALLETS_TOGGLE_RANDOM_WALLET = 'BENCHMARKS_WALLETS_TOGGLE_RANDOM_WALLET',
  BENCHMARKS_WALLETS_SELECT_WALLET = 'BENCHMARKS_WALLETS_SELECT_WALLET',
  BENCHMARKS_WALLETS_CHANGE_FEE = 'BENCHMARKS_WALLETS_CHANGE_FEE',
  BENCHMARKS_WALLETS_GET_ALL_TXS = 'BENCHMARKS_WALLETS_GET_ALL_TXS',
  BENCHMARKS_WALLETS_GET_ALL_TXS_SUCCESS = 'BENCHMARKS_GET_ALL_TXS_SUCCESS',
}

export const BENCHMARKS_WALLETS_CLOSE = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_CLOSE;
export const BENCHMARKS_WALLETS_GET_WALLETS = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_GET_WALLETS;
export const BENCHMARKS_WALLETS_GET_WALLETS_SUCCESS = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_GET_WALLETS_SUCCESS;
export const BENCHMARKS_WALLETS_UPDATE_WALLETS_SUCCESS = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_UPDATE_WALLETS_SUCCESS;
export const BENCHMARKS_WALLETS_CHANGE_TRANSACTION_BATCH = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_CHANGE_TRANSACTION_BATCH;
export const BENCHMARKS_WALLETS_SEND_TXS = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_SEND_TXS;
export const BENCHMARKS_WALLETS_SEND_TX_SYNCED = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_SEND_TX_SYNCED;
export const BENCHMARKS_WALLETS_SEND_TX_SUCCESS = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_SEND_TX_SUCCESS;
export const BENCHMARKS_WALLETS_TOGGLE_RANDOM_WALLET = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_TOGGLE_RANDOM_WALLET;
export const BENCHMARKS_WALLETS_SELECT_WALLET = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_SELECT_WALLET;
export const BENCHMARKS_WALLETS_CHANGE_FEE = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_CHANGE_FEE;
export const BENCHMARKS_WALLETS_GET_ALL_TXS = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_GET_ALL_TXS;
export const BENCHMARKS_WALLETS_GET_ALL_TXS_SUCCESS = BenchmarksWalletsActionTypes.BENCHMARKS_WALLETS_GET_ALL_TXS_SUCCESS;

export interface BenchmarksWalletsAction extends FeatureAction<BenchmarksWalletsActionTypes> {
  readonly type: BenchmarksWalletsActionTypes;
}

export class BenchmarksWalletsClose implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_CLOSE;
}

export class BenchmarksWalletsGetWallets implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_GET_WALLETS;

  constructor(public payload?: { initialRequest: boolean }) {}
}

export class BenchmarksWalletsGetWalletsSuccess implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_GET_WALLETS_SUCCESS;

  constructor(public payload: Pick<BenchmarksWallet, 'publicKey' | 'privateKey' | 'minaTokens' | 'nonce'>[]) {}
}

export class BenchmarksWalletsUpdateWalletsSuccess implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_UPDATE_WALLETS_SUCCESS;

  constructor(public payload: BenchmarksWallet[]) {}
}

export class BenchmarksWalletsChangeTransactionBatch implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_CHANGE_TRANSACTION_BATCH;

  constructor(public payload: number) {}
}

export class BenchmarksWalletsSendTxs implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_SEND_TXS;
}

export class BenchmarksWalletsSendTxSynced implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_SEND_TX_SYNCED;

  constructor(public payload: BenchmarksWalletTransaction[]) {}
}

export class BenchmarksWalletsSendTxSuccess implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_SEND_TX_SUCCESS;

  constructor(public payload: Partial<{ transactions: BenchmarksWalletTransaction[], error: Error }>) {}
}

export class BenchmarksWalletsToggleRandomWallet implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_TOGGLE_RANDOM_WALLET;
}

export class BenchmarksWalletsSelectWallet implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_SELECT_WALLET;

  constructor(public payload: BenchmarksWallet) {}
}

export class BenchmarksWalletsChangeFee implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_CHANGE_FEE;

  constructor(public payload: number) {}
}

export class BenchmarksWalletsGetAllTxs implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_GET_ALL_TXS;
}

export class BenchmarksWalletsGetAllTxsSuccess implements BenchmarksWalletsAction {
  readonly type = BENCHMARKS_WALLETS_GET_ALL_TXS_SUCCESS;

  constructor(public payload: {
    mempoolTxs: MempoolTransaction[],
    includedTxs: MempoolTransaction[],
  }) {}
}


export type BenchmarksWalletsActions =
  | BenchmarksWalletsClose
  | BenchmarksWalletsGetWallets
  | BenchmarksWalletsGetWalletsSuccess
  | BenchmarksWalletsUpdateWalletsSuccess
  | BenchmarksWalletsChangeTransactionBatch
  | BenchmarksWalletsSendTxs
  | BenchmarksWalletsSendTxSynced
  | BenchmarksWalletsSendTxSuccess
  | BenchmarksWalletsToggleRandomWallet
  | BenchmarksWalletsSelectWallet
  | BenchmarksWalletsChangeFee
  | BenchmarksWalletsGetAllTxs
  | BenchmarksWalletsGetAllTxsSuccess
  ;
