#![allow(non_snake_case)]

use crate::Dunewyrm::SelectHighestUtilityTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderSourceMode {
    SdslV,
    Wgsl,
    Hlsl,
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
    PreferredHlslAvailable,
    RequiredSdslVAvailable,
    RequiredWgslAvailable,
    RequiredHlslAvailable,
    ConflictingRequirements,
    ConflictingPreferences,
    NoShaderSourceFeasible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderSourceStrategyRequest {
    pub Label: String,
    pub SdslVSource: Option<String>,
    pub WgslSource: Option<String>,
    pub HlslSource: Option<String>,
    pub Constraints: ShaderSourceStrategyConstraints,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShaderSourceStrategyConstraints {
    pub AllowSdslV: bool,
    pub AllowWgsl: bool,
    pub AllowHlsl: bool,
    pub RequireSdslV: bool,
    pub RequireWgsl: bool,
    pub RequireHlsl: bool,
    pub PreferWgsl: bool,
    pub PreferHlsl: bool,
}

impl Default for ShaderSourceStrategyConstraints {
    fn default() -> Self {
        Self {
            AllowSdslV: true,
            AllowWgsl: true,
            AllowHlsl: true,
            RequireSdslV: false,
            RequireWgsl: false,
            RequireHlsl: false,
            PreferWgsl: false,
            PreferHlsl: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShaderSourceStrategyFeasibility {
    pub SdslVFeasible: bool,
    pub WgslFeasible: bool,
    pub HlslFeasible: bool,
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
    let c = request.Constraints;
    if [c.RequireSdslV, c.RequireWgsl, c.RequireHlsl]
        .iter()
        .filter(|x| **x)
        .count()
        > 1
    {
        return NoFeasible(
            ShaderSourceStrategyReason::ConflictingRequirements,
            vec![
                ShaderSourceMode::SdslV,
                ShaderSourceMode::Wgsl,
                ShaderSourceMode::Hlsl,
            ],
        );
    }
    if c.PreferWgsl && c.PreferHlsl {
        return NoFeasible(
            ShaderSourceStrategyReason::ConflictingPreferences,
            vec![ShaderSourceMode::Wgsl, ShaderSourceMode::Hlsl],
        );
    }

    let sdslv_has = HasNonEmptySource(request.SdslVSource.as_deref());
    let wgsl_has = HasNonEmptySource(request.WgslSource.as_deref());
    let hlsl_has = HasNonEmptySource(request.HlslSource.as_deref());

    let sdslv_feasible = c.AllowSdslV && sdslv_has && !c.RequireWgsl && !c.RequireHlsl;
    let wgsl_feasible = c.AllowWgsl && wgsl_has && !c.RequireSdslV && !c.RequireHlsl;
    let hlsl_feasible = c.AllowHlsl && hlsl_has && !c.RequireSdslV && !c.RequireWgsl;
    let feasibility = ShaderSourceStrategyFeasibility {
        SdslVFeasible: sdslv_feasible,
        WgslFeasible: wgsl_feasible,
        HlslFeasible: hlsl_feasible,
    };

    let mut rejected = Vec::new();
    MaybeRejectMode(
        ShaderSourceMode::SdslV,
        c.AllowSdslV,
        sdslv_has,
        c.RequireWgsl || c.RequireHlsl,
        sdslv_feasible,
        &mut rejected,
    );
    MaybeRejectMode(
        ShaderSourceMode::Wgsl,
        c.AllowWgsl,
        wgsl_has,
        c.RequireSdslV || c.RequireHlsl,
        wgsl_feasible,
        &mut rejected,
    );
    MaybeRejectMode(
        ShaderSourceMode::Hlsl,
        c.AllowHlsl,
        hlsl_has,
        c.RequireSdslV || c.RequireWgsl,
        hlsl_feasible,
        &mut rejected,
    );

    let mut scored = Vec::new();
    if sdslv_feasible {
        scored.push((
            ShaderSourceMode::SdslV,
            if c.PreferWgsl || c.PreferHlsl {
                0.6
            } else {
                1.0
            },
        ));
    }
    if wgsl_feasible {
        scored.push((
            ShaderSourceMode::Wgsl,
            if c.PreferWgsl { 1.0 } else { 0.75 },
        ));
    }
    if hlsl_feasible {
        scored.push((
            ShaderSourceMode::Hlsl,
            if c.PreferHlsl { 1.0 } else { 0.65 },
        ));
    }

    match SelectHighestUtilityTarget(&scored).map(|x| x.0) {
        Some(ShaderSourceMode::SdslV) => ShaderSourceStrategyDecision {
            SelectedMode: ShaderSourceMode::SdslV,
            Reason: if c.RequireSdslV {
                ShaderSourceStrategyReason::RequiredSdslVAvailable
            } else {
                ShaderSourceStrategyReason::PreferredSdslVAvailable
            },
            RejectedModes: rejected,
            Feasibility: feasibility,
        },
        Some(ShaderSourceMode::Wgsl) => ShaderSourceStrategyDecision {
            SelectedMode: ShaderSourceMode::Wgsl,
            Reason: if c.RequireWgsl {
                ShaderSourceStrategyReason::RequiredWgslAvailable
            } else {
                ShaderSourceStrategyReason::PreferredWgslAvailable
            },
            RejectedModes: rejected,
            Feasibility: feasibility,
        },
        Some(ShaderSourceMode::Hlsl) => ShaderSourceStrategyDecision {
            SelectedMode: ShaderSourceMode::Hlsl,
            Reason: if c.RequireHlsl {
                ShaderSourceStrategyReason::RequiredHlslAvailable
            } else {
                ShaderSourceStrategyReason::PreferredHlslAvailable
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

fn NoFeasible(
    reason: ShaderSourceStrategyReason,
    modes: Vec<ShaderSourceMode>,
) -> ShaderSourceStrategyDecision {
    ShaderSourceStrategyDecision {
        SelectedMode: ShaderSourceMode::NoShaderSourceFeasible,
        Reason: reason,
        RejectedModes: modes
            .into_iter()
            .map(|mode| RejectedShaderSourceMode {
                Mode: mode,
                Reason: ShaderSourceRejectedReason::BlockedByRequirement,
            })
            .collect(),
        Feasibility: ShaderSourceStrategyFeasibility {
            SdslVFeasible: false,
            WgslFeasible: false,
            HlslFeasible: false,
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
    blocked: bool,
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
    } else if blocked {
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
    fn Request(
        sdslv: Option<&str>,
        wgsl: Option<&str>,
        hlsl: Option<&str>,
    ) -> ShaderSourceStrategyRequest {
        ShaderSourceStrategyRequest {
            Label: "StrategyProbe".to_string(),
            SdslVSource: sdslv.map(|x| x.to_string()),
            WgslSource: wgsl.map(|x| x.to_string()),
            HlslSource: hlsl.map(|x| x.to_string()),
            Constraints: ShaderSourceStrategyConstraints::default(),
        }
    }

    #[test]
    fn HlslSelectionPolicyAndConflicts() {
        assert_eq!(
            SelectShaderSourceStrategy(&Request(Some("a"), Some("b"), Some("c"))).SelectedMode,
            ShaderSourceMode::SdslV
        );
        assert_eq!(
            SelectShaderSourceStrategy(&Request(
                None,
                None,
                Some("float4 PSMain():SV_Target{return 1;}")
            ))
            .SelectedMode,
            ShaderSourceMode::Hlsl
        );

        let mut prefer = Request(Some("a"), Some("b"), Some("c"));
        prefer.Constraints.PreferHlsl = true;
        assert_eq!(
            SelectShaderSourceStrategy(&prefer).SelectedMode,
            ShaderSourceMode::Hlsl
        );

        let mut need = Request(None, None, Some("c"));
        need.Constraints.RequireHlsl = true;
        assert_eq!(
            SelectShaderSourceStrategy(&need).SelectedMode,
            ShaderSourceMode::Hlsl
        );

        let mut need_missing = Request(Some("a"), None, None);
        need_missing.Constraints.RequireHlsl = true;
        assert_eq!(
            SelectShaderSourceStrategy(&need_missing).SelectedMode,
            ShaderSourceMode::NoShaderSourceFeasible
        );

        let mut dis = Request(None, None, Some("c"));
        dis.Constraints.AllowHlsl = false;
        let d = SelectShaderSourceStrategy(&dis);
        assert!(!d.Feasibility.HlslFeasible);

        let white = Request(None, None, Some(" \n\t"));
        assert_eq!(
            SelectShaderSourceStrategy(&white).SelectedMode,
            ShaderSourceMode::NoShaderSourceFeasible
        );

        let mut conflict = Request(Some("a"), Some("b"), Some("c"));
        conflict.Constraints.PreferWgsl = true;
        conflict.Constraints.PreferHlsl = true;
        assert_eq!(
            SelectShaderSourceStrategy(&conflict).Reason,
            ShaderSourceStrategyReason::ConflictingPreferences
        );
    }
}
