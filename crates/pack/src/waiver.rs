//! Waivers: a person letting a change through an obligation it failed.
//!
//! A waiver is not a discharge. Nobody checked the obligation again and found
//! it holds: someone with the authority the obligation names for a waiver
//! looked at the failure and decided the change may go ahead anyway. So a
//! waived obligation is `waived`, still outstanding, and a certificate carrying
//! one is `conditional` and names who decided and under what authority.
//!
//! A waiver covers one failure, exactly as the validator reported it, and no
//! other. Waiving "`aws_s3_bucket_acl.docs` grants public-read" lets that
//! bucket through; a change that also makes another bucket public fails the
//! obligation with another failure, which the waiver does not cover, and stays
//! `rejected`. An obligation naming no waiver authority cannot be waived at
//! all: the change has to be fixed, or the request it answers changed.
//!
//! Whether the person granting a waiver holds its authority is the caller's to
//! check, as the API checks a reviewer's scopes. What this module refuses is a
//! waiver no holder of any authority could grant here.

use capsulet_ir::Identifier;
use capsulet_ir::correctness::Identity;
use capsulet_ir::correctness::evidence::{EvidenceRef, RecordedTime};
use capsulet_ir::correctness::obligation::DischargeState;
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use serde::{Deserialize, Serialize};

use crate::{Decision, PackError};

/// The evidence id a waiver is cited under: `waivers/` and its id.
pub const WAIVER_PREFIX: &str = "waivers/";

/// A person's decision to let a change through a failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiver {
    /// Names the waiver. A waived obligation records it as its policy.
    pub id: Identifier,
    /// The obligation waived.
    pub obligation: Identifier,
    /// The authority it is granted under: the one the obligation names for a
    /// waiver.
    pub authority: Identifier,
    /// Who granted it.
    pub by: Identifier,
    /// Why, for whoever reads the certificate.
    pub reason: String,
    /// The failure it accepts, exactly as the validator reported it.
    pub failure: String,
}

/// Applies `waivers` to the obligations `decision` failed.
///
/// A waiver whose failure is the obligation's turns it `waived`, naming the
/// waiver, its authority and who granted it, and resting on the waiver itself,
/// which is cited as evidence under [`WAIVER_PREFIX`] so the certificate and
/// its bundle carry it. Returns the waivers that covered nothing: the
/// obligation did not fail, or failed some other way.
///
/// # Errors
///
/// Returns [`PackError::BadWaiver`], and changes nothing, when a waiver names
/// an obligation the decision does not hold, one that names no waiver
/// authority, or an authority other than the one it names.
pub fn waive(
    decision: &mut Decision,
    waivers: &[Waiver],
    at: RecordedTime,
) -> Result<Vec<Identifier>, PackError> {
    for waiver in waivers {
        let bad = |detail: String| PackError::BadWaiver {
            waiver: waiver.id.to_string(),
            detail,
        };
        let obligation = decision
            .obligations
            .iter()
            .find(|obligation| obligation.statement.id == waiver.obligation)
            .ok_or_else(|| {
                bad(format!(
                    "the pack states no obligation `{}`",
                    waiver.obligation
                ))
            })?;
        match &obligation.statement.waiver_authority {
            None => {
                return Err(bad(format!(
                    "`{}` names no waiver authority, so it cannot be waived",
                    waiver.obligation
                )));
            }
            Some(named) if named != &waiver.authority => {
                return Err(bad(format!(
                    "`{}` is waived under `{named}`, not `{}`",
                    waiver.obligation, waiver.authority
                )));
            }
            Some(_) => {}
        }
    }

    let mut unused = Vec::new();
    for waiver in waivers {
        let Some(obligation) = decision.obligations.iter_mut().find(|obligation| {
            obligation.statement.id == waiver.obligation
                && matches!(&obligation.state, DischargeState::Failed { reason, .. } if reason == &waiver.failure)
        }) else {
            unused.push(waiver.id.clone());
            continue;
        };
        let bytes = serde_json::to_vec(waiver).map_err(|error| PackError::BadWaiver {
            waiver: waiver.id.to_string(),
            detail: error.to_string(),
        })?;
        let id = Identifier::parse(format!("{WAIVER_PREFIX}{}", waiver.id)).map_err(|_| {
            PackError::BadWaiver {
                waiver: waiver.id.to_string(),
                detail: "its id cannot be cited".to_string(),
            }
        })?;
        let byte_length = bytes.len() as u64;
        let content = decision.blobs.insert(bytes);
        decision.evidence.push(EvidenceRef {
            id,
            content,
            media_type: "application/json".to_string(),
            byte_length,
            // A person has no version; the capacity they acted in is recorded
            // in its place.
            producer: Producer {
                kind: ProducerKind::Human,
                identity: Identity::new(waiver.by.clone(), waiver.authority.to_string()),
            },
            captured_at: at,
        });
        obligation.state = DischargeState::Waived {
            policy: waiver.id.clone(),
            authority: waiver.authority.clone(),
            by: Some(waiver.by.clone()),
            evidence: vec![content],
        };
    }
    Ok(unused)
}
