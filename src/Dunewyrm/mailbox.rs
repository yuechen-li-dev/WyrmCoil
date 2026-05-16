#![allow(non_snake_case)]

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DwMessagePayload {
    None,
    Bool(bool),
    I32(i32),
    F32(f32),
    PairI32 { A: i32, B: i32 },
}

impl Eq for DwMessagePayload {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DwMessage {
    pub Kind: u32,
    pub Payload: DwMessagePayload,
}

impl DwMessage {
    pub fn None(kind: u32) -> Self {
        Self {
            Kind: kind,
            Payload: DwMessagePayload::None,
        }
    }

    pub fn Bool(kind: u32, value: bool) -> Self {
        Self {
            Kind: kind,
            Payload: DwMessagePayload::Bool(value),
        }
    }

    pub fn I32(kind: u32, value: i32) -> Self {
        Self {
            Kind: kind,
            Payload: DwMessagePayload::I32(value),
        }
    }

    pub fn F32(kind: u32, value: f32) -> Self {
        Self {
            Kind: kind,
            Payload: DwMessagePayload::F32(value),
        }
    }

    pub fn PairI32(kind: u32, a: i32, b: i32) -> Self {
        Self {
            Kind: kind,
            Payload: DwMessagePayload::PairI32 { A: a, B: b },
        }
    }

    pub fn ValueI32Or(&self, default: i32) -> i32 {
        match self.Payload {
            DwMessagePayload::I32(value) => value,
            _ => default,
        }
    }
}

pub struct DwMailbox {
    Visible: VecDeque<DwMessage>,
    Staged: VecDeque<DwMessage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DwMailboxChunk {
    pub Visible: Vec<DwMessage>,
    pub Staged: Vec<DwMessage>,
}

impl DwMailbox {
    pub fn New() -> Self {
        Self {
            Visible: VecDeque::new(),
            Staged: VecDeque::new(),
        }
    }

    pub fn BeginTick(&mut self) {
        while let Some(message) = self.Staged.pop_front() {
            self.Visible.push_back(message);
        }
    }

    pub fn PeekFront(&self) -> Option<DwMessage> {
        self.Visible.front().copied()
    }

    pub fn ConsumeFront(&mut self) -> Option<DwMessage> {
        self.Visible.pop_front()
    }

    pub fn HasKind(&self, kind: u32) -> bool {
        self.Visible.iter().any(|message| message.Kind == kind)
    }

    pub fn PeekFirstKind(&self, kind: u32) -> Option<DwMessage> {
        self.Visible
            .iter()
            .find(|message| message.Kind == kind)
            .copied()
    }

    pub fn ConsumeFirstKind(&mut self, kind: u32) -> Option<DwMessage> {
        let index = self
            .Visible
            .iter()
            .position(|message| message.Kind == kind)?;
        self.Visible.remove(index)
    }

    pub fn ConsumeAllKind(&mut self, kind: u32) -> Vec<DwMessage> {
        let mut matches = Vec::new();
        let mut kept = VecDeque::new();

        while let Some(message) = self.Visible.pop_front() {
            if message.Kind == kind {
                matches.push(message);
            } else {
                kept.push_back(message);
            }
        }

        self.Visible = kept;
        matches
    }

    pub fn Enqueue(&mut self, message: DwMessage) {
        self.Staged.push_back(message);
    }

    pub fn EnqueueVisibleForTest(&mut self, message: DwMessage) {
        self.Visible.push_back(message);
    }

    pub fn VisibleSnapshot(&self) -> Vec<DwMessage> {
        self.Visible.iter().copied().collect()
    }

    pub fn StagedSnapshot(&self) -> Vec<DwMessage> {
        self.Staged.iter().copied().collect()
    }

    pub fn ExportChunk(&self) -> DwMailboxChunk {
        DwMailboxChunk {
            Visible: self.VisibleSnapshot(),
            Staged: self.StagedSnapshot(),
        }
    }

    pub fn FromChunk(chunk: DwMailboxChunk) -> Self {
        Self {
            Visible: chunk.Visible.into(),
            Staged: chunk.Staged.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn PayloadConstructorsCoverTypedVariants() {
        assert_eq!(
            DwMessage::None(1).Payload,
            DwMessagePayload::None,
            "expected none payload constructor to set None"
        );
        assert_eq!(
            DwMessage::Bool(2, true).Payload,
            DwMessagePayload::Bool(true),
            "expected bool payload constructor to preserve bool value"
        );
        assert_eq!(
            DwMessage::I32(3, 5).Payload,
            DwMessagePayload::I32(5),
            "expected i32 payload constructor to preserve i32 value"
        );
        assert_eq!(
            DwMessage::F32(4, 1.5).Payload,
            DwMessagePayload::F32(1.5),
            "expected f32 payload constructor to preserve f32 value"
        );
        assert_eq!(
            DwMessage::PairI32(5, 7, 9).Payload,
            DwMessagePayload::PairI32 { A: 7, B: 9 },
            "expected pair payload constructor to preserve both fields"
        );
    }

    #[test]
    fn FilterHelpersOnlyTouchVisibleAndPreserveFifo() {
        let mut mailbox = DwMailbox::New();
        mailbox.EnqueueVisibleForTest(DwMessage::I32(1, 11));
        mailbox.EnqueueVisibleForTest(DwMessage::I32(2, 22));
        mailbox.EnqueueVisibleForTest(DwMessage::Bool(1, false));
        mailbox.Enqueue(DwMessage::I32(1, 99));

        assert!(
            mailbox.HasKind(1),
            "expected HasKind to scan visible messages"
        );
        assert!(
            !mailbox.HasKind(9),
            "expected HasKind to return false for unknown kind"
        );

        assert_eq!(
            mailbox.PeekFirstKind(1),
            Some(DwMessage::I32(1, 11)),
            "expected PeekFirstKind to return first visible matching message"
        );
        assert_eq!(
            mailbox.PeekFront(),
            Some(DwMessage::I32(1, 11)),
            "expected PeekFirstKind to avoid consuming messages"
        );

        assert_eq!(
            mailbox.ConsumeFirstKind(1),
            Some(DwMessage::I32(1, 11)),
            "expected ConsumeFirstKind to consume only first matching message"
        );
        assert_eq!(
            mailbox.VisibleSnapshot(),
            vec![DwMessage::I32(2, 22), DwMessage::Bool(1, false)],
            "expected non-matching visible messages to keep relative order"
        );

        assert_eq!(
            mailbox.ConsumeAllKind(1),
            vec![DwMessage::Bool(1, false)],
            "expected ConsumeAllKind to return remaining kind-matching messages in FIFO order"
        );
        assert_eq!(
            mailbox.VisibleSnapshot(),
            vec![DwMessage::I32(2, 22)],
            "expected ConsumeAllKind to retain non-matching visible messages in FIFO order"
        );
        assert_eq!(
            mailbox.StagedSnapshot(),
            vec![DwMessage::I32(1, 99)],
            "expected staged messages to remain untouched by visible helper operations"
        );
    }

    #[test]
    fn ChunkRoundTripPreservesTypedPayloadsAndPromotion() {
        let mut mailbox = DwMailbox::New();
        mailbox.EnqueueVisibleForTest(DwMessage::Bool(10, true));
        mailbox.Enqueue(DwMessage::F32(11, 2.5));

        let chunk = mailbox.ExportChunk();
        assert_eq!(
            chunk.Visible,
            vec![DwMessage::Bool(10, true)],
            "expected visible typed payloads to survive export chunk"
        );
        assert_eq!(
            chunk.Staged,
            vec![DwMessage::F32(11, 2.5)],
            "expected staged typed payloads to survive export chunk"
        );

        let mut restored = DwMailbox::FromChunk(chunk);
        assert_eq!(
            restored.PeekFront(),
            Some(DwMessage::Bool(10, true)),
            "expected restored visible queue to preserve typed payload and order"
        );
        restored.BeginTick();
        assert_eq!(
            restored.VisibleSnapshot(),
            vec![DwMessage::Bool(10, true), DwMessage::F32(11, 2.5)],
            "expected staged typed messages to promote on BeginTick in FIFO order"
        );
    }
}
