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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DwUtilitySelectionReason {
    HighestScore,
    TieBreakFirst,
    NoCandidates,
    NoPositiveScore,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DwUtilityCandidateReport<T: Copy> {
    pub Target: T,
    pub RawScore: f32,
    pub ClampedScore: f32,
    pub Rank: usize,
    pub IsSelected: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DwUtilityDecisionReport<T: Copy> {
    pub Selected: Option<T>,
    pub Candidates: Vec<DwUtilityCandidateReport<T>>,
    pub TieCount: usize,
    pub TieBreakApplied: bool,
    pub SelectionReason: DwUtilitySelectionReason,
}

pub fn SelectHighestUtilityTargetWithReport<T: Copy>(
    scored: &[(T, f32)],
) -> DwUtilityDecisionReport<T> {
    if scored.is_empty() {
        return DwUtilityDecisionReport {
            Selected: None,
            Candidates: Vec::new(),
            TieCount: 0,
            TieBreakApplied: false,
            SelectionReason: DwUtilitySelectionReason::NoCandidates,
        };
    }

    let mut clamped_candidates = Vec::with_capacity(scored.len());
    for (target, raw_score) in scored {
        clamped_candidates.push((*target, *raw_score, raw_score.clamp(0.0, 1.0)));
    }

    let mut best_index = 0_usize;
    let mut best_score = clamped_candidates[0].1;
    for (index, candidate) in clamped_candidates.iter().enumerate().skip(1) {
        if candidate.1 > best_score {
            best_index = index;
            best_score = candidate.1;
        }
    }

    let tie_count = clamped_candidates
        .iter()
        .filter(|candidate| (candidate.1 - best_score).abs() <= f32::EPSILON)
        .count();

    let mut candidates = Vec::with_capacity(clamped_candidates.len());
    for (index, (target, raw_score, clamped_score)) in clamped_candidates.iter().enumerate() {
        let rank = clamped_candidates
            .iter()
            .filter(|other| other.1 > *raw_score)
            .count();
        candidates.push(DwUtilityCandidateReport {
            Target: *target,
            RawScore: *raw_score,
            ClampedScore: *clamped_score,
            Rank: rank,
            IsSelected: index == best_index,
        });
    }

    DwUtilityDecisionReport {
        Selected: Some(clamped_candidates[best_index].0),
        Candidates: candidates,
        TieCount: tie_count,
        TieBreakApplied: tie_count > 1,
        SelectionReason: if best_score <= 0.0 {
            DwUtilitySelectionReason::NoPositiveScore
        } else if tie_count > 1 {
            DwUtilitySelectionReason::TieBreakFirst
        } else {
            DwUtilitySelectionReason::HighestScore
        },
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ReportSelectsHighestScoreAndMarksCandidateFlags() {
        let report = SelectHighestUtilityTargetWithReport(&[('A', 0.2), ('B', 0.9), ('C', 0.7)]);
        assert_eq!(
            report.Selected,
            Some('B'),
            "highest score candidate should be selected"
        );
        assert_eq!(report.TieCount, 1, "single winner should not report tie");
        assert_eq!(
            report.SelectionReason,
            DwUtilitySelectionReason::HighestScore,
            "positive untied winner should report HighestScore reason"
        );
        assert!(
            report.Candidates[1].IsSelected,
            "selected candidate flag should be true for winner"
        );
        assert!(
            !report.Candidates[0].IsSelected && !report.Candidates[2].IsSelected,
            "non-selected candidates should report IsSelected=false"
        );
        assert_eq!(
            report.Candidates[1].RawScore, 0.9,
            "raw score should be retained"
        );
        assert_eq!(
            report.Candidates[1].ClampedScore, 0.9,
            "clamped score should reflect score clamping rule"
        );
    }

    #[test]
    fn ReportTieBehaviorIsDeterministicAndUsesFirst() {
        let report = SelectHighestUtilityTargetWithReport(&[('A', 0.8), ('B', 0.8), ('C', 0.2)]);
        assert_eq!(
            report.Selected,
            Some('A'),
            "ties should select first input candidate"
        );
        assert_eq!(
            report.TieCount, 2,
            "tie count should include all top-score candidates"
        );
        assert!(report.TieBreakApplied, "ties should mark tie-break applied");
        assert_eq!(
            report.SelectionReason,
            DwUtilitySelectionReason::TieBreakFirst,
            "tie selection reason should report first-candidate tie break"
        );
    }

    #[test]
    fn ReportEmptyCandidatesMarksNoCandidatesReason() {
        let report = SelectHighestUtilityTargetWithReport::<char>(&[]);
        assert_eq!(
            report.Selected, None,
            "empty input should not select a candidate"
        );
        assert_eq!(
            report.SelectionReason,
            DwUtilitySelectionReason::NoCandidates,
            "empty input should report NoCandidates reason"
        );
        assert!(
            SelectHighestUtilityTarget::<char>(&[]).is_none(),
            "legacy selector should remain unchanged for empty candidates"
        );
    }

    #[test]
    fn ReportNegativeAndZeroScoresRemainCompatible() {
        let input = [('A', -0.2), ('B', 0.0), ('C', -1.0)];
        let report = SelectHighestUtilityTargetWithReport(&input);
        let old = SelectHighestUtilityTarget(&input);
        assert_eq!(
            report.Selected,
            Some('B'),
            "highest raw score should remain selected"
        );
        assert_eq!(
            old,
            Some(('B', 0.0)),
            "legacy selector output should remain unchanged"
        );
        assert_eq!(
            report.SelectionReason,
            DwUtilitySelectionReason::NoPositiveScore,
            "zero-or-negative winner should report NoPositiveScore"
        );
        assert_eq!(
            report.Candidates[0].ClampedScore, 0.0,
            "report should include clamped score diagnostics"
        );
    }

    #[test]
    fn ReportIsDeterministicAcrossRepeatedRuns() {
        let input = [('A', 0.4), ('B', 0.9), ('C', 0.9), ('D', -0.5)];
        let first = SelectHighestUtilityTargetWithReport(&input);
        let second = SelectHighestUtilityTargetWithReport(&input);
        assert_eq!(first, second, "same input should produce identical report");
        assert_eq!(
            SelectHighestUtilityTarget(&input),
            Some(('B', 0.9)),
            "legacy selector should still resolve ties by first candidate"
        );
    }
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
