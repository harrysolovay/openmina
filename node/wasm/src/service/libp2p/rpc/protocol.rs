//! The definition of a request/response protocol via inbound
//! and outbound substream upgrades. The inbound upgrade
//! receives a request and sends a response, whereas the
//! outbound upgrade send a request and receives a response.

use lib::p2p::rpc::{P2pRpcId, P2pRpcIncomingId, P2pRpcRequest, P2pRpcResponse};
use libp2p::core::upgrade::{InboundUpgrade, OutboundUpgrade, UpgradeInfo};
use libp2p::futures::{channel::oneshot, future::BoxFuture, prelude::*};
use libp2p::swarm::NegotiatedSubstream;
use std::{fmt, io};

/// Response substream upgrade protocol.
///
/// Receives a request and sends a response.
#[derive(Debug)]
pub struct ResponseProtocol {
    pub(crate) request_sender: oneshot::Sender<(P2pRpcIncomingId, P2pRpcRequest)>,
    pub(crate) response_receiver: oneshot::Receiver<P2pRpcResponse>,
    pub(crate) request_id: P2pRpcIncomingId,
}

impl UpgradeInfo for ResponseProtocol {
    type Info = &'static str;
    type InfoIter = std::iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once(super::RPC_PROTOCOL_NAME)
    }
}

impl InboundUpgrade<NegotiatedSubstream> for ResponseProtocol {
    type Output = bool;
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_inbound(
        mut self,
        mut io: NegotiatedSubstream,
        protocol: Self::Info,
    ) -> Self::Future {
        // TODO(biner): incoming rpcs.
        todo!()
        // async move {
        //     let read = self.codec.read_request(&protocol, &mut io);
        //     let request = read.await?;
        //     match self.request_sender.send((self.request_id, request)) {
        //         Ok(()) => {},
        //         Err(_) => panic!(
        //             "Expect request receiver to be alive i.e. protocol handler to be alive.",
        //         ),
        //     }

        //     if let Ok(response) = self.response_receiver.await {
        //         let write = self.codec.write_response(&protocol, &mut io, response);
        //         write.await?;

        //         io.close().await?;
        //         // Response was sent. Indicate to handler to emit a `ResponseSent` event.
        //         Ok(true)
        //     } else {
        //         io.close().await?;
        //         // No response was sent. Indicate to handler to emit a `ResponseOmission` event.
        //         Ok(false)
        //     }
        // }.boxed()
    }
}

/// Request substream upgrade protocol.
///
/// Sends a request and receives a response.
pub struct RequestProtocol {
    pub(crate) request_id: P2pRpcId,
    pub(crate) request: P2pRpcRequest,
}

impl fmt::Debug for RequestProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestProtocol")
            .field("request_id", &self.request_id)
            .finish()
    }
}

impl UpgradeInfo for RequestProtocol {
    type Info = &'static str;
    type InfoIter = std::iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        std::iter::once(super::RPC_PROTOCOL_NAME)
    }
}

impl OutboundUpgrade<NegotiatedSubstream> for RequestProtocol {
    type Output = P2pRpcResponse;
    type Error = io::Error;
    type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn upgrade_outbound(
        mut self,
        mut io: NegotiatedSubstream,
        protocol: Self::Info,
    ) -> Self::Future {
        async move {
            shared::log::info!(shared::log::system_time(); kind = "WriteReq");
            let mut encoded = vec![];
            self.request.write_msg(self.request_id, &mut encoded)?;

            const PREFIX: &'static [u8] =
                b"\x07\x00\x00\x00\x00\x00\x00\x00\x02\xfd\x52\x50\x43\x00\x01";
            io.write_all(PREFIX).await?;
            io.write_all(b"\x1a\x00\x00\x00\x00\x00\x00\x00\x01")
                .await?;
            io.write_all(&encoded).await?;
            io.flush().await?;

            let mut prefix = [0; PREFIX.len()];
            io.read_exact(&mut prefix).await?;

            // ignore heartbeats
            for _ in 0..16 {
                let mut b = [0; 9];
                io.read_exact(&mut b).await?;
                if b != [1, 0, 0, 0, 0, 0, 0, 0, 0] {
                    break;
                }
            }

            if &prefix != PREFIX {
                return Err(io::Error::new(io::ErrorKind::Other, "RPC prefix mismatch"));
            }

            P2pRpcResponse::async_read_msg(self.request.kind(), &mut io)
                .await
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
        }
        .boxed()
    }
}
