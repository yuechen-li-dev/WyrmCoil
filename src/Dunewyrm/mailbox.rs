#![allow(non_snake_case)]

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DwMessage {
    pub Kind: u32,
    pub Value: i32,
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
