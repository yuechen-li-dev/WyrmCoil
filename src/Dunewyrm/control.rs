#![allow(non_snake_case)]

use crate::{DwDecisionCommitState, DwDecisionTraceEntry, DwFrameCtx, DwFrameId, DwPhase};

pub type DwScoreFn = fn(&DwFrameCtx) -> f32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DwTieBreak {
    KeepCurrent,
    First,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DwDecideOptions {
    pub Hysteresis: f32,
    pub MinCommitTicks: u32,
    pub TieBreak: DwTieBreak,
}

#[derive(Clone, Copy, Debug)]
pub struct DwUtilityCandidate {
    pub Target: DwFrameId,
    pub Score: DwScoreFn,
}

pub fn SelectHighestUtilityTarget<T: Copy>(scored: &[(T, f32)]) -> Option<(T, f32)> {
    if scored.is_empty() {
        return None;
    }

    let mut best = scored[0];
    for candidate in scored.iter().skip(1) {
        if candidate.1 > best.1 {
            best = *candidate;
        }
    }

    Some(best)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DwControl {
    Continue { Pc: u32 },
    WaitTicks { Ticks: u32, Pc: u32 },
    Push { Target: DwFrameId, ResumePc: u32 },
    Pop,
    Replace { Target: DwFrameId },
    Stay,
    Complete,
    Fail { Reason: &'static str },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DwControlSummary {
    Continue,
    WaitTicks { Ticks: u32 },
    Push,
    Pop,
    Replace,
    Stay,
    Complete,
    Fail,
}

pub mod Dw {
    use super::{
        DwControl, DwDecideOptions, DwDecisionCommitState, DwDecisionTraceEntry, DwFrameCtx,
        DwFrameId, DwPhase, DwScoreFn, DwTieBreak, DwUtilityCandidate, SelectHighestUtilityTarget,
    };

    pub fn When(target: DwFrameId, scorer: DwScoreFn) -> DwUtilityCandidate {
        DwUtilityCandidate {
            Target: target,
            Score: scorer,
        }
    }

    pub fn Decide(
        ctx: &mut DwFrameCtx,
        candidates: &[DwUtilityCandidate],
        options: DwDecideOptions,
    ) -> DwControl {
        if candidates.is_empty() {
            return Fail("decide candidates cannot be empty");
        }

        let mut scored = Vec::new();
        for candidate in candidates {
            let raw_score = (candidate.Score)(ctx);
            let clamped = raw_score.clamp(0.0, 1.0);
            scored.push((candidate.Target, clamped));
        }

        let key = ctx.DecisionKey();
        let current_index = ctx.FindDecisionMemoryIndex(key);
        let current_state = current_index.map(|index| ctx.DecisionMemoryAt(index));

        let (raw_best, raw_best_score) =
            SelectHighestUtilityTarget(&scored).expect("scored candidates should not be empty");

        let mut selected = raw_best;
        let mut tie_break_applied = false;
        let mut min_commit_applied = false;
        let mut hysteresis_applied = false;

        let tied_targets = scored
            .iter()
            .filter(|(_, score)| (*score - raw_best_score).abs() <= f32::EPSILON)
            .map(|(target, _)| *target)
            .collect::<Vec<_>>();

        if tied_targets.len() > 1 {
            match options.TieBreak {
                DwTieBreak::KeepCurrent => {
                    if let Some(current) = current_state {
                        if tied_targets.contains(&current.Target) {
                            selected = current.Target;
                            tie_break_applied = true;
                        } else {
                            selected = tied_targets[0];
                        }
                    } else {
                        selected = tied_targets[0];
                    }
                }
                DwTieBreak::First => {
                    selected = tied_targets[0];
                    tie_break_applied = true;
                }
            }
        }

        if let Some(current) = current_state {
            let current_score = scored
                .iter()
                .find(|(target, _)| *target == current.Target)
                .map(|(_, score)| *score);

            if current.Age < options.MinCommitTicks {
                if current_score.is_some() {
                    selected = current.Target;
                    min_commit_applied = true;
                }
            } else if let Some(curr_score) = current_score {
                let selected_score = scored
                    .iter()
                    .find(|(target, _)| *target == selected)
                    .map(|(_, score)| *score)
                    .unwrap_or(0.0);
                if selected != current.Target && selected_score < curr_score + options.Hysteresis {
                    selected = current.Target;
                    hysteresis_applied = true;
                }
            }
        }

        let next_age = if let Some(current) = current_state {
            if current.Target == selected {
                current.Age.saturating_add(1)
            } else {
                0
            }
        } else {
            0
        };

        ctx.UpsertDecisionMemory(DwDecisionCommitState {
            Frame: key.Frame,
            Pc: key.Pc,
            Target: selected,
            Age: next_age,
        });

        ctx.RecordDecision(DwDecisionTraceEntry {
            Tick: ctx.Tick(),
            Frame: key.Frame,
            Pc: key.Pc,
            Candidates: scored,
            RawWinner: raw_best,
            Selected: selected,
            TieBreakApplied: tie_break_applied,
            MinCommitApplied: min_commit_applied,
            HysteresisApplied: hysteresis_applied,
            CommitAge: next_age,
        });

        DwControl::Push {
            Target: selected,
            ResumePc: ctx.Pc(),
        }
    }

    pub fn Continue<P: DwPhase>(phase: P) -> DwControl {
        DwControl::Continue { Pc: phase.ToPc() }
    }

    pub fn WaitTicks<P: DwPhase>(ticks: u32, phase: P) -> DwControl {
        DwControl::WaitTicks {
            Ticks: ticks,
            Pc: phase.ToPc(),
        }
    }

    pub fn Push<P: DwPhase>(target: DwFrameId, resume_phase: P) -> DwControl {
        DwControl::Push {
            Target: target,
            ResumePc: resume_phase.ToPc(),
        }
    }

    pub fn Pop() -> DwControl {
        DwControl::Pop
    }

    pub fn Replace(target: DwFrameId) -> DwControl {
        DwControl::Replace { Target: target }
    }

    pub fn Stay() -> DwControl {
        DwControl::Stay
    }

    pub fn Complete() -> DwControl {
        DwControl::Complete
    }

    pub fn Fail(reason: &'static str) -> DwControl {
        DwControl::Fail { Reason: reason }
    }
}
