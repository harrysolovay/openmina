use std::sync::{Arc, Mutex};

use crate::{VerifierIndex, VerifierSRS};

use super::{SnarkBlockVerifyId, VerifiableBlockWithHash};

pub trait SnarkBlockVerifyService: redux::Service {
    fn verify_init(
        &mut self,
        req_id: SnarkBlockVerifyId,
        verifier_index: Arc<VerifierIndex>,
        verifier_srs: Arc<Mutex<VerifierSRS>>,
        block: VerifiableBlockWithHash,
    );
}
