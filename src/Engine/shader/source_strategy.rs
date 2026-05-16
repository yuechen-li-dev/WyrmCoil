#![allow(non_snake_case)]

use crate::Dunewyrm::SelectHighestUtilityTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderSourceMode {
    SdslV,
    Wgsl,
    NoShaderSourceFeasible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderSourceRejectedReason {
    SourceMissing,
    DisabledByConstraints,
    BlockedByRequirement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderSourceStrategyReason {
    PreferredSdslVAvailable,
    PreferredWgslAvailable,
    RequiredSdslVAvailable,
    RequiredWgslAvailable,
    ConflictingRequirements,
    NoShaderSourceFeasible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderSourceStrategyRequest {
    pub Label: String,
    pub SdslVSource: Option<String>,
    pub WgslSource: Option<String>,
    pub Constraints: ShaderSourceStrategyConstraints,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShaderSourceStrategyConstraints {
    pub AllowSdslV: bool,
    pub AllowWgsl: bool,
    pub RequireSdslV: bool,
    pub RequireWgsl: bool,
    pub PreferWgsl: bool,
}

impl Default for ShaderSourceStrategyConstraints {
    fn default() -> Self {
        Self {
            AllowSdslV: true,
            AllowWgsl: true,
            RequireSdslV: false,
            RequireWgsl: false,
            PreferWgsl: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShaderSourceStrategyFeasibility {
    pub SdslVFeasible: bool,
    pub WgslFeasible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RejectedShaderSourceMode {
    pub Mode: ShaderSourceMode,
    pub Reason: ShaderSourceRejectedReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderSourceStrategyDecision {
    pub SelectedMode: ShaderSourceMode,
    pub Reason: ShaderSourceStrategyReason,
    pub RejectedModes: Vec<RejectedShaderSourceMode>,
    pub Feasibility: ShaderSourceStrategyFeasibility,
}

pub fn SelectShaderSourceStrategy(
    request: &ShaderSourceStrategyRequest,
) -> ShaderSourceStrategyDecision {
    let constraints = request.Constraints;
    if constraints.RequireSdslV && constraints.RequireWgsl {
        return ShaderSourceStrategyDecision {
            SelectedMode: ShaderSourceMode::NoShaderSourceFeasible,
            Reason: ShaderSourceStrategyReason::ConflictingRequirements,
            RejectedModes: vec![
                RejectedShaderSourceMode {
                    Mode: ShaderSourceMode::SdslV,
                    Reason: ShaderSourceRejectedReason::BlockedByRequirement,
                },
                RejectedShaderSourceMode {
                    Mode: ShaderSourceMode::Wgsl,
                    Reason: ShaderSourceRejectedReason::BlockedByRequirement,
                },
            ],
            Feasibility: ShaderSourceStrategyFeasibility {
                SdslVFeasible: false,
                WgslFeasible: false,
            },
        };
    }

    let sdslv_has_source = HasNonEmptySource(request.SdslVSource.as_deref());
    let wgsl_has_source = HasNonEmptySource(request.WgslSource.as_deref());

    let sdslv_feasible = constraints.AllowSdslV
        && sdslv_has_source
        && (!constraints.RequireWgsl || constraints.RequireSdslV);
    let wgsl_feasible = constraints.AllowWgsl
        && wgsl_has_source
        && (!constraints.RequireSdslV || constraints.RequireWgsl);

    let feasibility = ShaderSourceStrategyFeasibility {
        SdslVFeasible: sdslv_feasible,
        WgslFeasible: wgsl_feasible,
    };

    let mut rejected = Vec::new();
    MaybeRejectMode(
        ShaderSourceMode::SdslV,
        constraints.AllowSdslV,
        sdslv_has_source,
        constraints.RequireWgsl,
        sdslv_feasible,
        &mut rejected,
    );
    MaybeRejectMode(
        ShaderSourceMode::Wgsl,
        constraints.AllowWgsl,
        wgsl_has_source,
        constraints.RequireSdslV,
        wgsl_feasible,
        &mut rejected,
    );

    let mut scored_modes = Vec::new();
    if sdslv_feasible {
        scored_modes.push((
            ShaderSourceMode::SdslV,
            if constraints.PreferWgsl { 0.8 } else { 1.0 },
        ));
    }
    if wgsl_feasible {
        scored_modes.push((
            ShaderSourceMode::Wgsl,
            if constraints.PreferWgsl { 1.0 } else { 0.7 },
        ));
    }

    let selected = SelectHighestUtilityTarget(&scored_modes).map(|entry| entry.0);
    match selected {
        Some(ShaderSourceMode::SdslV) => ShaderSourceStrategyDecision {
            SelectedMode: ShaderSourceMode::SdslV,
            Reason: if constraints.RequireSdslV {
                ShaderSourceStrategyReason::RequiredSdslVAvailable
            } else {
                ShaderSourceStrategyReason::PreferredSdslVAvailable
            },
            RejectedModes: rejected,
            Feasibility: feasibility,
        },
        Some(ShaderSourceMode::Wgsl) => ShaderSourceStrategyDecision {
            SelectedMode: ShaderSourceMode::Wgsl,
            Reason: if constraints.RequireWgsl {
                ShaderSourceStrategyReason::RequiredWgslAvailable
            } else {
                ShaderSourceStrategyReason::PreferredWgslAvailable
            },
            RejectedModes: rejected,
            Feasibility: feasibility,
        },
        _ => ShaderSourceStrategyDecision {
            SelectedMode: ShaderSourceMode::NoShaderSourceFeasible,
            Reason: ShaderSourceStrategyReason::NoShaderSourceFeasible,
            RejectedModes: rejected,
            Feasibility: feasibility,
        },
    }
}

fn HasNonEmptySource(source: Option<&str>) -> bool {
    source.map(|text| !text.trim().is_empty()).unwrap_or(false)
}

fn MaybeRejectMode(
    mode: ShaderSourceMode,
    allowed: bool,
    has_source: bool,
    blocked_by_requirement: bool,
    feasible: bool,
    rejected: &mut Vec<RejectedShaderSourceMode>,
) {
    if feasible {
        return;
    }
    let reason = if !allowed {
        ShaderSourceRejectedReason::DisabledByConstraints
    } else if !has_source {
        ShaderSourceRejectedReason::SourceMissing
    } else if blocked_by_requirement {
        ShaderSourceRejectedReason::BlockedByRequirement
    } else {
        ShaderSourceRejectedReason::SourceMissing
    };
    rejected.push(RejectedShaderSourceMode {
        Mode: mode,
        Reason: reason,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn Request(sdslv: Option<&str>, wgsl: Option<&str>) -> ShaderSourceStrategyRequest {
        ShaderSourceStrategyRequest {
            Label: "StrategyProbe".to_string(),
            SdslVSource: sdslv.map(|x| x.to_string()),
            WgslSource: wgsl.map(|x| x.to_string()),
            Constraints: ShaderSourceStrategyConstraints::default(),
        }
    }

    #[test]
    fn DefaultPreferenceSelectsSdslVWhenBothAvailable() {
        let d =
            SelectShaderSourceStrategy(&Request(Some("shader S {}"), Some("@vertex fn vs() {}")));
        assert_eq!(
            d.SelectedMode,
            ShaderSourceMode::SdslV,
            "default policy should prefer SDSL-V when both are feasible"
        );
        assert_eq!(
            d.Reason,
            ShaderSourceStrategyReason::PreferredSdslVAvailable,
            "reason should report preferred SDSL-V selection"
        );
    }

    #[test]
    fn PreferWgslSelectsWgslWhenBothAvailable() {
        let mut req = Request(Some("shader S {}"), Some("@vertex fn vs() {}"));
        req.Constraints.PreferWgsl = true;
        let d = SelectShaderSourceStrategy(&req);
        assert_eq!(
            d.SelectedMode,
            ShaderSourceMode::Wgsl,
            "prefer-wgsl should override default SDSL-V preference when both are feasible"
        );
    }

    #[test]
    fn SingleSourceAvailabilitySelectsFeasiblePath() {
        let only_sdslv = SelectShaderSourceStrategy(&Request(Some("shader S {}"), None));
        assert_eq!(
            only_sdslv.SelectedMode,
            ShaderSourceMode::SdslV,
            "SDSL-V should be selected when it is the only feasible source"
        );

        let only_wgsl = SelectShaderSourceStrategy(&Request(None, Some("@vertex fn vs() {}")));
        assert_eq!(
            only_wgsl.SelectedMode,
            ShaderSourceMode::Wgsl,
            "WGSL should be selected when it is the only feasible source"
        );
    }

    #[test]
    fn DisabledConstraintsRejectDisabledModeAndSelectFallback() {
        let mut sdslv_disabled = Request(Some("shader S {}"), Some("@vertex fn vs() {}"));
        sdslv_disabled.Constraints.AllowSdslV = false;
        let d1 = SelectShaderSourceStrategy(&sdslv_disabled);
        assert_eq!(
            d1.SelectedMode,
            ShaderSourceMode::Wgsl,
            "WGSL should be selected when SDSL-V is disabled but WGSL remains feasible"
        );
        assert!(
            d1.RejectedModes
                .iter()
                .any(|x| x.Mode == ShaderSourceMode::SdslV
                    && x.Reason == ShaderSourceRejectedReason::DisabledByConstraints),
            "SDSL-V disabled rejection should be recorded"
        );

        let mut wgsl_disabled = Request(Some("shader S {}"), Some("@vertex fn vs() {}"));
        wgsl_disabled.Constraints.AllowWgsl = false;
        let d2 = SelectShaderSourceStrategy(&wgsl_disabled);
        assert_eq!(
            d2.SelectedMode,
            ShaderSourceMode::SdslV,
            "SDSL-V should be selected when WGSL is disabled but SDSL-V remains feasible"
        );
        assert!(
            d2.RejectedModes
                .iter()
                .any(|x| x.Mode == ShaderSourceMode::Wgsl
                    && x.Reason == ShaderSourceRejectedReason::DisabledByConstraints),
            "WGSL disabled rejection should be recorded"
        );
    }

    #[test]
    fn RequiredConstraintsAndConflictsProduceHardFailure() {
        let mut need_sdslv = Request(None, Some("@vertex fn vs() {}"));
        need_sdslv.Constraints.RequireSdslV = true;
        let d1 = SelectShaderSourceStrategy(&need_sdslv);
        assert_eq!(
            d1.SelectedMode,
            ShaderSourceMode::NoShaderSourceFeasible,
            "requiring SDSL-V should reject WGSL-only inputs"
        );

        let mut need_wgsl = Request(Some("shader S {}"), None);
        need_wgsl.Constraints.RequireWgsl = true;
        let d2 = SelectShaderSourceStrategy(&need_wgsl);
        assert_eq!(
            d2.SelectedMode,
            ShaderSourceMode::NoShaderSourceFeasible,
            "requiring WGSL should reject SDSL-V-only inputs"
        );

        let mut conflict = Request(Some("shader S {}"), Some("@vertex fn vs() {}"));
        conflict.Constraints.RequireSdslV = true;
        conflict.Constraints.RequireWgsl = true;
        let d3 = SelectShaderSourceStrategy(&conflict);
        assert_eq!(
            d3.SelectedMode,
            ShaderSourceMode::NoShaderSourceFeasible,
            "conflicting strict requirements should be a hard failure"
        );
        assert_eq!(
            d3.Reason,
            ShaderSourceStrategyReason::ConflictingRequirements,
            "conflicting requirements should produce structured conflict reason"
        );
    }

    #[test]
    fn EmptyOrWhitespaceSourceIsNotFeasible() {
        let d = SelectShaderSourceStrategy(&Request(Some("   \n\t"), Some("")));
        assert_eq!(
            d.SelectedMode,
            ShaderSourceMode::NoShaderSourceFeasible,
            "whitespace-only or empty source should not be considered feasible"
        );
        assert!(
            !d.Feasibility.SdslVFeasible && !d.Feasibility.WgslFeasible,
            "feasibility flags should mark both sources infeasible for empty/whitespace input"
        );
    }

    #[test]
    fn RejectedModesAndFeasibilityReflectMissingAndBlocked() {
        let mut req = Request(Some("shader S {}"), Some("@vertex fn vs() {}"));
        req.Constraints.RequireSdslV = true;
        let d = SelectShaderSourceStrategy(&req);
        assert_eq!(
            d.SelectedMode,
            ShaderSourceMode::SdslV,
            "required SDSL-V should be selected when available"
        );
        assert!(
            d.RejectedModes
                .iter()
                .any(|x| x.Mode == ShaderSourceMode::Wgsl
                    && x.Reason == ShaderSourceRejectedReason::BlockedByRequirement),
            "WGSL should be rejected as blocked by SDSL-V requirement"
        );
        assert!(
            d.Feasibility.SdslVFeasible,
            "SDSL-V feasibility should remain true when required and source is present"
        );
        assert!(
            !d.Feasibility.WgslFeasible,
            "WGSL feasibility should be false when blocked by requirement"
        );
    }

    #[test]
    fn UsesDunewyrmUtilitySelectionScoringPolicy() {
        let default_pick = SelectHighestUtilityTarget(&[
            (ShaderSourceMode::SdslV, 1.0),
            (ShaderSourceMode::Wgsl, 0.7),
        ]);
        assert_eq!(
            default_pick,
            Some((ShaderSourceMode::SdslV, 1.0)),
            "default scoring should keep SDSL-V above WGSL"
        );

        let prefer_wgsl_pick = SelectHighestUtilityTarget(&[
            (ShaderSourceMode::SdslV, 0.8),
            (ShaderSourceMode::Wgsl, 1.0),
        ]);
        assert_eq!(
            prefer_wgsl_pick,
            Some((ShaderSourceMode::Wgsl, 1.0)),
            "prefer-wgsl scoring should raise WGSL above SDSL-V"
        );
    }
}
