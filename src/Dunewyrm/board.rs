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

pub trait DwBoardValue: Copy + PartialEq {
    fn Kind() -> DwBoardKind;
    fn TryGetFrom(board: &DwBoard, slot: u32) -> Option<Self>;
    fn SetOn(board: &mut DwBoard, slot: u32, value: Self);
    fn ToSnapshotValue(value: Self) -> DwBoardValueSnapshot;
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DwBoardValueSnapshot {
    Bool(bool),
    I32(i32),
    F32(f32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct DwBoardSnapshotEntry {
    pub Slot: u32,
    pub Name: &'static str,
    pub Kind: DwBoardKind,
    pub Value: DwBoardValueSnapshot,
    pub Dirty: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DwBoardSnapshot {
    pub Entries: Vec<DwBoardSnapshotEntry>,
    pub DirtySlots: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DwBoardTtlEntry {
    pub Slot: u32,
    pub RemainingTicks: u32,
    pub ExpireValue: DwBoardValueSnapshot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DwBoardTtlSnapshot {
    pub Entries: Vec<DwBoardTtlEntry>,
}

pub struct DwBoard {
    BoolEntries: Vec<(u32, bool)>,
    I32Entries: Vec<(u32, i32)>,
    F32Entries: Vec<(u32, f32)>,
    DirtySlots: BTreeSet<u32>,
    SlotMeta: HashMap<u32, DwSlotMetaChunk>,
    LastSlotCollision: Option<DwSlotCollision>,
    TtlEntries: Vec<DwBoardTtlEntry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DwBoardChunk {
    pub BoolEntries: Vec<(u32, bool)>,
    pub I32Entries: Vec<(u32, i32)>,
    pub F32Entries: Vec<(u32, f32)>,
    pub DirtySlots: Vec<u32>,
    pub SlotMeta: Vec<(u32, DwSlotMetaChunk)>,
    pub LastSlotCollision: Option<DwSlotCollision>,
    pub TtlEntries: Vec<DwBoardTtlEntry>,
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
            TtlEntries: Vec::new(),
        }
    }

    pub fn Set<T: DwBoardValue>(&mut self, key: DwKey<T>, value: T) -> Result<(), DwSlotCollision> {
        self.LastSlotCollision = None;
        self.ValidateKeyMeta(key.Name, key.Slot, T::Kind())?;
        let changed = self.TryGet(key) != Some(value);
        T::SetOn(self, key.Slot, value);
        if changed {
            self.DirtySlots.insert(key.Slot);
        }
        Ok(())
    }

    pub fn SetBoolWithTtl(
        &mut self,
        key: DwKey<bool>,
        value: bool,
        ttl_ticks: u32,
    ) -> Result<(), DwSlotCollision> {
        self.SetWithTtlInternal(key, value, ttl_ticks, false)
    }
    pub fn SetI32WithTtl(
        &mut self,
        key: DwKey<i32>,
        value: i32,
        ttl_ticks: u32,
        expire_to: i32,
    ) -> Result<(), DwSlotCollision> {
        self.SetWithTtlInternal(key, value, ttl_ticks, expire_to)
    }
    pub fn SetF32WithTtl(
        &mut self,
        key: DwKey<f32>,
        value: f32,
        ttl_ticks: u32,
        expire_to: f32,
    ) -> Result<(), DwSlotCollision> {
        self.SetWithTtlInternal(key, value, ttl_ticks, expire_to)
    }

    fn SetWithTtlInternal<T: DwBoardValue>(
        &mut self,
        key: DwKey<T>,
        value: T,
        ttl_ticks: u32,
        expire_to: T,
    ) -> Result<(), DwSlotCollision> {
        self.Set(key, value)?;
        if ttl_ticks == 0 {
            self.ClearTtlBySlot(key.Slot);
            self.Set(key, expire_to)?;
            return Ok(());
        }
        self.SetTtlEntry(key.Slot, ttl_ticks, expire_to);
        Ok(())
    }

    pub fn TickTtl(&mut self) {
        let mut remaining = Vec::new();
        let ttl_entries = std::mem::take(&mut self.TtlEntries);
        for mut entry in ttl_entries {
            if entry.RemainingTicks > 0 {
                entry.RemainingTicks -= 1;
            }
            if entry.RemainingTicks == 0 {
                let current = self.TryGetBySlot(entry.Slot, entry.ExpireValue);
                if current != Some(entry.ExpireValue) {
                    self.SetBySlot(entry.Slot, entry.ExpireValue);
                    self.DirtySlots.insert(entry.Slot);
                }
            } else {
                remaining.push(entry);
            }
        }
        self.TtlEntries = remaining;
    }

    pub fn ClearTtl<T: DwBoardValue>(&mut self, key: DwKey<T>) {
        self.ClearTtlBySlot(key.Slot);
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

    pub fn Snapshot(&self) -> DwBoardSnapshot {
        let mut entries = Vec::new();
        let mut slots = self.SlotMeta.keys().copied().collect::<Vec<_>>();
        slots.sort_unstable();
        for slot in slots {
            let meta = self
                .SlotMeta
                .get(&slot)
                .expect("slot metadata should exist");
            let value = match meta.Kind {
                DwBoardKind::Bool => DwBoardValueSnapshot::Bool(
                    self.TryGet(DwKey::<bool>::New(meta.Name, slot))
                        .unwrap_or(false),
                ),
                DwBoardKind::I32 => DwBoardValueSnapshot::I32(
                    self.TryGet(DwKey::<i32>::New(meta.Name, slot)).unwrap_or(0),
                ),
                DwBoardKind::F32 => DwBoardValueSnapshot::F32(
                    self.TryGet(DwKey::<f32>::New(meta.Name, slot))
                        .unwrap_or(0.0),
                ),
            };
            entries.push(DwBoardSnapshotEntry {
                Slot: slot,
                Name: meta.Name,
                Kind: meta.Kind,
                Value: value,
                Dirty: self.DirtySlots.contains(&slot),
            });
        }
        DwBoardSnapshot {
            Entries: entries,
            DirtySlots: self.DirtySlots(),
        }
    }

    pub fn TtlSnapshot(&self) -> DwBoardTtlSnapshot {
        let mut entries = self.TtlEntries.clone();
        entries.sort_by_key(|entry| entry.Slot);
        DwBoardTtlSnapshot { Entries: entries }
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
            TtlEntries: self.TtlEntries.clone(),
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
            TtlEntries: chunk.TtlEntries,
        }
    }

    fn SetTtlEntry<T: DwBoardValue>(&mut self, slot: u32, ttl_ticks: u32, expire_to: T) {
        let expire_value = T::ToSnapshotValue(expire_to);
        if let Some(entry) = self.TtlEntries.iter_mut().find(|entry| entry.Slot == slot) {
            entry.RemainingTicks = ttl_ticks;
            entry.ExpireValue = expire_value;
        } else {
            self.TtlEntries.push(DwBoardTtlEntry {
                Slot: slot,
                RemainingTicks: ttl_ticks,
                ExpireValue: expire_value,
            });
        }
    }
    fn ClearTtlBySlot(&mut self, slot: u32) {
        self.TtlEntries.retain(|entry| entry.Slot != slot);
    }

    fn TryGetBySlot(
        &self,
        slot: u32,
        value_kind: DwBoardValueSnapshot,
    ) -> Option<DwBoardValueSnapshot> {
        match value_kind {
            DwBoardValueSnapshot::Bool(_) => self
                .BoolEntries
                .iter()
                .find(|entry| entry.0 == slot)
                .map(|entry| DwBoardValueSnapshot::Bool(entry.1)),
            DwBoardValueSnapshot::I32(_) => self
                .I32Entries
                .iter()
                .find(|entry| entry.0 == slot)
                .map(|entry| DwBoardValueSnapshot::I32(entry.1)),
            DwBoardValueSnapshot::F32(_) => self
                .F32Entries
                .iter()
                .find(|entry| entry.0 == slot)
                .map(|entry| DwBoardValueSnapshot::F32(entry.1)),
        }
    }

    fn SetBySlot(&mut self, slot: u32, value: DwBoardValueSnapshot) {
        match value {
            DwBoardValueSnapshot::Bool(v) => bool::SetOn(self, slot, v),
            DwBoardValueSnapshot::I32(v) => i32::SetOn(self, slot, v),
            DwBoardValueSnapshot::F32(v) => f32::SetOn(self, slot, v),
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
    fn ToSnapshotValue(value: Self) -> DwBoardValueSnapshot {
        DwBoardValueSnapshot::Bool(value)
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
    fn ToSnapshotValue(value: Self) -> DwBoardValueSnapshot {
        DwBoardValueSnapshot::I32(value)
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
    fn ToSnapshotValue(value: Self) -> DwBoardValueSnapshot {
        DwBoardValueSnapshot::F32(value)
    }
}
