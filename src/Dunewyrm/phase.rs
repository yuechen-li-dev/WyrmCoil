#![allow(non_snake_case)]

use std::collections::{BTreeSet, HashMap};
use std::marker::PhantomData;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DwBoardKind {
    Bool,
    I32,
    F32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DwKey<T: DwBoardValue> {
    pub Name: &'static str,
    pub Slot: u32,
    Marker: PhantomData<T>,
}

impl<T: DwBoardValue> DwKey<T> {
    pub const fn New(name: &'static str, slot: u32) -> Self {
        Self {
            Name: name,
            Slot: slot,
            Marker: PhantomData,
        }
    }
}

pub trait DwBoardValue: Copy {
    fn Kind() -> DwBoardKind;
    fn TryGetFrom(board: &DwBoard, slot: u32) -> Option<Self>;
    fn SetOn(board: &mut DwBoard, slot: u32, value: Self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DwSlotMetaChunk {
    Name: &'static str,
    Kind: DwBoardKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DwSlotCollision {
    pub Slot: u32,
    pub ExistingName: &'static str,
    pub ExistingKind: DwBoardKind,
    pub IncomingName: &'static str,
    pub IncomingKind: DwBoardKind,
}

pub struct DwBoard {
    BoolEntries: Vec<(u32, bool)>,
    I32Entries: Vec<(u32, i32)>,
    F32Entries: Vec<(u32, f32)>,
    DirtySlots: BTreeSet<u32>,
    SlotMeta: HashMap<u32, DwSlotMetaChunk>,
    LastSlotCollision: Option<DwSlotCollision>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DwBoardChunk {
    pub BoolEntries: Vec<(u32, bool)>,
    pub I32Entries: Vec<(u32, i32)>,
    pub F32Entries: Vec<(u32, f32)>,
    pub DirtySlots: Vec<u32>,
    pub SlotMeta: Vec<(u32, DwSlotMetaChunk)>,
    pub LastSlotCollision: Option<DwSlotCollision>,
}

impl DwBoard {
    pub fn New() -> Self {
        Self {
            BoolEntries: Vec::new(),
            I32Entries: Vec::new(),
            F32Entries: Vec::new(),
            DirtySlots: BTreeSet::new(),
            SlotMeta: HashMap::new(),
            LastSlotCollision: None,
        }
    }

    pub fn Set<T: DwBoardValue>(&mut self, key: DwKey<T>, value: T) -> Result<(), DwSlotCollision> {
        self.LastSlotCollision = None;
        self.ValidateKeyMeta(key.Name, key.Slot, T::Kind())?;
        T::SetOn(self, key.Slot, value);
        self.DirtySlots.insert(key.Slot);
        Ok(())
    }

    pub fn TryGet<T: DwBoardValue>(&self, key: DwKey<T>) -> Option<T> {
        T::TryGetFrom(self, key.Slot)
    }

    pub fn GetOr<T: DwBoardValue>(&self, key: DwKey<T>, fallback: T) -> T {
        self.TryGet(key).unwrap_or(fallback)
    }

    pub fn IsDirty<T: DwBoardValue>(&self, key: DwKey<T>) -> bool {
        self.DirtySlots.contains(&key.Slot)
    }

    pub fn DirtySlots(&self) -> Vec<u32> {
        self.DirtySlots.iter().copied().collect()
    }

    pub fn ClearDirty(&mut self) {
        self.DirtySlots.clear();
    }

    pub fn LastSlotCollision(&self) -> Option<DwSlotCollision> {
        self.LastSlotCollision
    }

    pub fn ExportChunk(&self) -> DwBoardChunk {
        let mut slot_meta = self
            .SlotMeta
            .iter()
            .map(|(slot, meta)| (*slot, *meta))
            .collect::<Vec<_>>();
        slot_meta.sort_by_key(|entry| entry.0);

        DwBoardChunk {
            BoolEntries: self.BoolEntries.clone(),
            I32Entries: self.I32Entries.clone(),
            F32Entries: self.F32Entries.clone(),
            DirtySlots: self.DirtySlots(),
            SlotMeta: slot_meta,
            LastSlotCollision: self.LastSlotCollision,
        }
    }

    pub fn FromChunk(chunk: DwBoardChunk) -> Self {
        let mut dirty = BTreeSet::new();
        for slot in chunk.DirtySlots {
            dirty.insert(slot);
        }
        let mut slot_meta = HashMap::new();
        for (slot, meta) in chunk.SlotMeta {
            slot_meta.insert(slot, meta);
        }

        Self {
            BoolEntries: chunk.BoolEntries,
            I32Entries: chunk.I32Entries,
            F32Entries: chunk.F32Entries,
            DirtySlots: dirty,
            SlotMeta: slot_meta,
            LastSlotCollision: chunk.LastSlotCollision,
        }
    }

    fn ValidateKeyMeta(
        &mut self,
        incoming_name: &'static str,
        incoming_slot: u32,
        incoming_kind: DwBoardKind,
    ) -> Result<(), DwSlotCollision> {
        if let Some(existing) = self.SlotMeta.get(&incoming_slot).copied() {
            if existing.Name == incoming_name && existing.Kind == incoming_kind {
                return Ok(());
            }
            let collision = DwSlotCollision {
                Slot: incoming_slot,
                ExistingName: existing.Name,
                ExistingKind: existing.Kind,
                IncomingName: incoming_name,
                IncomingKind: incoming_kind,
            };
            self.LastSlotCollision = Some(collision);
            return Err(collision);
        }

        self.SlotMeta.insert(
            incoming_slot,
            DwSlotMetaChunk {
                Name: incoming_name,
                Kind: incoming_kind,
            },
        );
        Ok(())
    }
}

impl DwBoardValue for bool {
    fn Kind() -> DwBoardKind {
        DwBoardKind::Bool
    }
    fn TryGetFrom(board: &DwBoard, slot: u32) -> Option<Self> {
        board
            .BoolEntries
            .iter()
            .find(|entry| entry.0 == slot)
            .map(|entry| entry.1)
    }
    fn SetOn(board: &mut DwBoard, slot: u32, value: Self) {
        if let Some(entry) = board.BoolEntries.iter_mut().find(|entry| entry.0 == slot) {
            entry.1 = value;
        } else {
            board.BoolEntries.push((slot, value));
        }
    }
}

impl DwBoardValue for i32 {
    fn Kind() -> DwBoardKind {
        DwBoardKind::I32
    }
    fn TryGetFrom(board: &DwBoard, slot: u32) -> Option<Self> {
        board
            .I32Entries
            .iter()
            .find(|entry| entry.0 == slot)
            .map(|entry| entry.1)
    }
    fn SetOn(board: &mut DwBoard, slot: u32, value: Self) {
        if let Some(entry) = board.I32Entries.iter_mut().find(|entry| entry.0 == slot) {
            entry.1 = value;
        } else {
            board.I32Entries.push((slot, value));
        }
    }
}

impl DwBoardValue for f32 {
    fn Kind() -> DwBoardKind {
        DwBoardKind::F32
    }
    fn TryGetFrom(board: &DwBoard, slot: u32) -> Option<Self> {
        board
            .F32Entries
            .iter()
            .find(|entry| entry.0 == slot)
            .map(|entry| entry.1)
    }
    fn SetOn(board: &mut DwBoard, slot: u32, value: Self) {
        if let Some(entry) = board.F32Entries.iter_mut().find(|entry| entry.0 == slot) {
            entry.1 = value;
        } else {
            board.F32Entries.push((slot, value));
        }
    }
}
