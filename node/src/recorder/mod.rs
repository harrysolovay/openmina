mod recorder;
pub use recorder::Recorder;

mod replayer;
pub use replayer::StateWithInputActionsReader;

use std::{
    borrow::Cow,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{Action, ActionKind, ActionWithMeta, State};

fn initial_state_path<P: AsRef<Path>>(path: P) -> PathBuf {
    path.as_ref().join("initial_state.cbor")
}

fn actions_path<P: AsRef<Path>>(path: P, file_index: usize) -> PathBuf {
    path.as_ref()
        .join(format!("actions_{}.cbor", file_index))
}

#[derive(Serialize, Deserialize)]
pub struct RecordedInitialState<'a> {
    pub rng_seed: u64,
    pub state: Cow<'a, State>,
}

impl<'a> RecordedInitialState<'a> {
    pub fn write_to<W: Write>(&self, writer: &mut W) -> serde_cbor::Result<()> {
        serde_cbor::to_writer(writer, self)
    }

    pub fn decode(encoded: &[u8]) -> serde_cbor::Result<Self> {
        serde_cbor::from_slice(encoded)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RecordedActionWithMeta<'a> {
    pub kind: ActionKind,
    pub meta: redux::ActionMeta,
    pub action: Option<Cow<'a, Action>>,
}

impl<'a> RecordedActionWithMeta<'a> {
    pub fn encode(&self) -> serde_cbor::Result<Vec<u8>> {
        serde_cbor::to_vec(self)
    }

    pub fn decode(encoded: &[u8]) -> serde_cbor::Result<Self> {
        serde_cbor::from_slice(encoded)
    }

    pub fn as_action_with_meta(self) -> Result<ActionWithMeta, Self> {
        if self.action.is_some() {
            let action = self.action.unwrap().into_owned();
            Ok(self.meta.with_action(action))
        } else {
            Err(self)
        }
    }
}

impl<'a> From<&'a ActionWithMeta> for RecordedActionWithMeta<'a> {
    fn from(value: &'a ActionWithMeta) -> Self {
        Self {
            kind: value.action().kind(),
            meta: value.meta().clone(),
            action: Some(Cow::Borrowed(value.action())),
        }
    }
}

impl From<(ActionKind, redux::ActionMeta)> for RecordedActionWithMeta<'static> {
    fn from((kind, meta): (ActionKind, redux::ActionMeta)) -> Self {
        Self {
            kind,
            meta,
            action: None,
        }
    }
}
