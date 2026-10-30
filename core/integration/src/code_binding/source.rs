//! Authorized exact retained-source coordination (corrective r2, findings
//! 2/3/7).
//!
//! `code_fetch_exact*` answers ONLY for a repository this scope durably
//! bound, ONLY for its published active generation or a generation covered by
//! a live read lease (an explicitly authorized pinned historical read), and
//! ONLY with bytes an ADAPTER read from retained custody and verified against
//! the durable indexed-source descriptor. The caller is authenticated through
//! the same `CodeBindingAuthorizer` as every other code operation, and the
//! resolution runs inside the coordinator's consistent-read window so a
//! publication race retries once or surfaces `GenerationChanged` instead of
//! returning an unchecked view. Nothing in this chain consults a checkout.

use engram_domain::{Actor, RepoId, Scope};
use engram_knowledge::{
    CodeReadLease, CoreError, CoreResult, ExactSourceView, RetainedSourceReader,
};
use std::sync::Arc;
use std::time::Duration;

use super::{CodeAuthorityAction, CodeBindingCoordinator};

/// How long an exact-source consistent-read window pins the view.
const EXACT_READ_WINDOW: Duration = Duration::from_secs(30);

/// One matched row's IMMUTABLE input snapshot for the same-operation exact
/// read (corrective r5, A4): copied from the caller's listing BEFORE the
/// operation starts and never re-read afterwards. Every decision inside the
/// operation (participant selection, symbol spans, span content validation)
/// works from these snapshots and the DURABLE records — never from mutable
/// row state re-read mid-operation.
#[derive(Debug, Clone)]
pub struct ExactReadCandidate {
    /// The row's durable id — the ONLY identity the operation trusts.
    pub record_id: String,
    /// The row's claimed line span, if any.
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    /// Entity names + location anchor (symbol matching inputs).
    pub names: Vec<String>,
    /// The row's first stored text line (span content validation).
    pub text_head: String,
}

/// The outcome of the same-operation bound-source read. Routing only — the
/// caller never receives partial state: `Unbound` means the caller's
/// ordinary paths apply, `Denied` is terminal (bound code NEVER falls back
/// to verbatim/stitching), `Served` is the validated whole answer.
#[derive(Debug)]
pub enum ParticipantExactSource {
    /// No matched record carries durable lineage: not a bound path.
    Unbound,
    /// The bound path denies: no current canonical participants, an
    /// ambiguous multi-generation origin, a failed symbol selection, or a
    /// participant whose claimed span does not match the retained revision
    /// (a changed path/span on a canonical-ID row). `symbol_selection` marks
    /// denials caused by the requested symbol so callers can phrase them.
    Denied {
        symbol_selection: bool,
        reason: String,
    },
    /// Authorized, digest-validated exact bytes plus the revision-bound
    /// participant spans, all resolved inside ONE consistent-read window.
    Served {
        repo_id: String,
        generation_id: String,
        /// The FULL retained-blob view (no output cap applied by this layer).
        view: ExactSourceView,
        /// Byte span of the selected symbol within `view.bytes`.
        span: Option<(usize, usize)>,
        /// The AUTHENTICATED participants: each durable record id with its
        /// durable canonical line span and the VALIDATED byte range of that
        /// span inside `view.bytes`. The ONLY rows/bytes a caller may
        /// consume for any (explicitly best-effort) fallback presentation
        /// of this bound path — mutable stored chunk text is never an
        /// authenticated payload (r5 C4, verification finding 1).
        participants: Vec<BoundParticipant>,
    },
}

/// One AUTHENTICATED participant of a bound exact read (r5 C4,
/// verification finding 1): the durable record identity plus the DURABLE
/// canonical line span and its validated byte range inside the served
/// `ExactSourceView::bytes`. A validated participant ID is not a validated
/// payload — bound fallback presentation (chunks mode) reconstructs text
/// EXCLUSIVELY from these authenticated retained bytes, never from the
/// mutable stored chunk text.
#[derive(Debug, Clone)]
pub struct BoundParticipant {
    pub record_id: String,
    /// Durable canonical span start (1-based, validated in-window).
    pub start_line: u32,
    /// Durable canonical span end (1-based, inclusive, validated in-window).
    pub end_line: u32,
    /// Byte range of `[start_line, end_line]` inside `view.bytes` — the
    /// authenticated payload bytes for this participant.
    pub byte_span: (usize, usize),
}

/// Internal immutable participant descriptor (D3): exists only INSIDE this
/// module's operations — deliberately not a public DTO. Carries BOTH the
/// caller's claimed span (mutable row state) and the DURABLE canonical
/// path/span copied from the pinned window's view (final r5 finding 2) —
/// participation is authenticated by comparing the two, never by trusting
/// the claim.
#[derive(Debug, Clone)]
struct ParticipantDescriptor {
    record_id: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
    names: Vec<String>,
    text_head: String,
    /// The record's DURABLE canonical source path (from the pinned view).
    canonical_path: String,
    /// The record's DURABLE canonical line span (from the pinned view).
    canonical_span: Option<engram_domain::CanonicalSpan>,
}

/// Internal routing classification of the candidate set against one view.
#[derive(Debug)]
enum ParticipantRouting {
    Unbound,
    WithoutCurrent {
        lineage_record_ids: Vec<String>,
    },
    /// Current durable lineage whose CANONICAL SOURCE PATH differs from the
    /// requested path (final r5 finding 2): repository/generation match is
    /// not path identity — such rows deny instead of participating, and the
    /// durable ownership means the request can never fall back to ordinary
    /// verbatim/stitching.
    ForeignPath {
        record_ids: Vec<String>,
    },
    Ambiguous {
        origins: Vec<(String, String)>,
    },
    Bound {
        repo_id: String,
        generation_id: String,
        descriptors: Vec<ParticipantDescriptor>,
    },
}

/// Canonical path comparison form: the same normalization MCP applies
/// (`./` prefix and trailing `/` stripped) so a durable staged path and a
/// caller request compare by identity, not by spelling.
fn normalize_source_path(path: &str) -> &str {
    path.trim_start_matches("./").trim_end_matches('/')
}

fn foreign_path_reason(record_ids: &[String]) -> String {
    format!(
        "matched chunks carry current durable lineage whose canonical source \
         path differs from the requested path (records: {record_ids:?})"
    )
}

fn route_against_view(
    view: &engram_retrieval::BoundCodeView,
    path: &str,
    candidates: &[ExactReadCandidate],
) -> ParticipantRouting {
    let requested_path = normalize_source_path(path);
    let published: std::collections::BTreeSet<(String, String)> = view
        .eligible_bindings()
        .into_iter()
        .map(|binding| (binding.repo_id, binding.generation_id))
        .collect();
    let mut descriptors = Vec::new();
    let mut origins: Vec<(String, String)> = Vec::new();
    let mut noncurrent: Vec<String> = Vec::new();
    let mut foreign_path: Vec<String> = Vec::new();
    for candidate in candidates {
        // POSITIVE durable ownership (review r5, final finding 1) is
        // `is_code_record` — lineage membership INCLUDING retired records.
        // It is deliberately distinct from CURRENT participation
        // (`code_lineage_of`, which excludes retired records). A row with
        // NO durable lineage (unstamped or forged-stamp) is not an
        // admissible participant (stamps are never authority).
        if !view.is_code_record(candidate.record_id.as_str()) {
            continue;
        }
        // A retirement fact is POSITIVE ownership with denied eligibility:
        // the path is bound code whose participant is retired — it DENIES
        // (never `Unbound`, so no ordinary verbatim/stitching fallback can
        // ever resurrect it). Skipping it here would misclassify a
        // retired-only unstamped row as unbound, reopening the
        // retired-code resurrection path.
        if view.is_retired_record(candidate.record_id.as_str()) {
            noncurrent.push(candidate.record_id.clone());
            continue;
        }
        let Some((repo, generation)) = view.code_lineage_of(candidate.record_id.as_str()) else {
            // Unreachable for a non-retired code record, kept fail-closed:
            // a record with durable lineage that resolves to no current
            // association is treated as noncurrent ownership, not unbound.
            noncurrent.push(candidate.record_id.clone());
            continue;
        };
        let origin = (repo.to_owned(), generation.to_owned());
        if !published.contains(&origin) {
            // Durable lineage on a NON-published generation (a retained old
            // generation): not a participant — but it proves the path is
            // bound code, so it can never fall through to an ordinary
            // fallback later.
            noncurrent.push(candidate.record_id.clone());
            continue;
        }
        // Canonical PATH identity (final r5 finding 2): the routing
        // association carries repository/generation, but a current-lineage
        // row whose durable canonical source path differs from the
        // requested path is a moved/impostor row for THIS path — it denies
        // even when repository and generation match. The durable path is
        // copied from the pinned view (never from the mutable row).
        let canonical_path = view
            .code_source_path_of(candidate.record_id.as_str())
            .to_owned();
        if normalize_source_path(&canonical_path) != requested_path {
            foreign_path.push(candidate.record_id.clone());
            continue;
        }
        let canonical_span = view.code_span_of(candidate.record_id.as_str());
        if !origins.contains(&origin) {
            origins.push(origin);
        }
        descriptors.push(ParticipantDescriptor {
            record_id: candidate.record_id.clone(),
            start_line: candidate.start_line,
            end_line: candidate.end_line,
            names: candidate.names.clone(),
            text_head: candidate.text_head.clone(),
            canonical_path,
            canonical_span,
        });
    }
    if !foreign_path.is_empty() {
        return ParticipantRouting::ForeignPath {
            record_ids: foreign_path,
        };
    }
    match origins.as_slice() {
        [] => {
            if noncurrent.is_empty() {
                ParticipantRouting::Unbound
            } else {
                ParticipantRouting::WithoutCurrent {
                    lineage_record_ids: noncurrent,
                }
            }
        }
        [only] => ParticipantRouting::Bound {
            repo_id: only.0.clone(),
            generation_id: only.1.clone(),
            descriptors,
        },
        _ => ParticipantRouting::Ambiguous { origins },
    }
}

/// Maps 1-based inclusive line numbers to byte offsets over the retained
/// blob (line separators `\n` and `\r\n`).
fn line_start_offsets(blob: &[u8]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(64);
    offsets.push(0);
    for (index, byte) in blob.iter().enumerate() {
        if *byte == b'\n' {
            offsets.push(index + 1);
        }
    }
    offsets
}

/// Computes the requested symbol's byte span AND the authenticated
/// participant payload spans from the participant descriptors, VALIDATING
/// every participant's claimed span against the DURABLE canonical span
/// staged at publication (final r5 finding 2): a canonical-ID row whose
/// claimed span does not EQUAL its durable span denies — current
/// durable-ID membership alone never authenticates a mutable row's span
/// claim, and mutable stored text is never consulted for span identity
/// (the C2-era "first line found anywhere inside the claimed span"
/// substring check authenticated neither the span's start nor its
/// width). Malformed claims (missing, zero start, reversed, beyond-file
/// end) reject; they are never normalized or clamped. Every authenticated
/// participant carries the byte range of its DURABLE span inside the
/// retained blob — the only payload bytes a fallback consumer may emit
/// (r5 C4, verification finding 1: a validated ID is not a validated
/// payload).
fn participant_span(
    descriptors: &[ParticipantDescriptor],
    symbol: Option<&str>,
    blob: &[u8],
) -> Result<(Option<(usize, usize)>, Vec<BoundParticipant>), (bool, String)> {
    let offsets = line_start_offsets(blob);
    // The revision's line count — the exclusive bound every claimed and
    // durable span end is validated against BEFORE any byte offset is
    // computed (final r5 finding 2: out-of-range ends REJECT, never clamp).
    // A blob ending in `\n` has `offsets.len() - 1` lines (the final entry
    // is EOF); one without a trailing newline has `offsets.len()` lines
    // (the last line runs to EOF).
    let line_count = if blob.last() == Some(&b'\n') || blob.is_empty() {
        offsets.len().saturating_sub(1)
    } else {
        offsets.len()
    } as u32;
    // Byte span of a VALIDATED (start, end): both within [1, line_count].
    // For `end == line_count` on a file without a trailing newline the line
    // genuinely extends to EOF — `blob.len()` is that line's exact end, not
    // a clamp (every bound was validated against `line_count` above).
    let span_bytes = |start_line: u32, end_line: u32| -> Option<(usize, usize)> {
        let start = *offsets.get(start_line.checked_sub(1)? as usize)?;
        let end = offsets
            .get(end_line as usize)
            .copied()
            .unwrap_or(blob.len());
        (start <= end && end <= blob.len()).then_some((start, end))
    };
    // Span identity (final r5 finding 2): every participant is validated
    // against its DURABLE canonical span — nothing skips validation. The
    // claimed bounds must be present and well-formed (malformed bounds
    // REJECT: start 0, reversed, end beyond the revision are never
    // normalized or clamped), and the claim must EQUAL the durable span
    // recorded at publication (a first-line substring anywhere in an
    // enlarged span authenticates nothing — mutable text is not consulted
    // for span identity at all, so missing spans and empty first lines
    // cannot skip this check either: both fail claim equality).
    let mut authenticated: Vec<(&ParticipantDescriptor, BoundParticipant)> =
        Vec::with_capacity(descriptors.len());
    for descriptor in descriptors {
        let (Some(claimed_start), Some(claimed_end)) = (descriptor.start_line, descriptor.end_line)
        else {
            return Err((
                false,
                format!(
                    "participant '{}' claims no line span — a participant without a \
                     claimed span cannot be authenticated against its durable canonical span",
                    descriptor.record_id
                ),
            ));
        };
        if claimed_start == 0 {
            return Err((
                false,
                format!(
                    "participant '{}' claims a malformed span (start line 0 — lines are \
                     1-based)",
                    descriptor.record_id
                ),
            ));
        }
        if claimed_end < claimed_start {
            return Err((
                false,
                format!(
                    "participant '{}' claims a malformed span (end line {claimed_end} before \
                     start line {claimed_start} — reversed bounds reject, they are never \
                     normalized)",
                    descriptor.record_id
                ),
            ));
        }
        if claimed_end > line_count {
            return Err((
                false,
                format!(
                    "participant '{}' claims a span beyond the retained revision (end line \
                     {claimed_end} > {line_count} lines — out-of-range bounds reject, they \
                     are never clamped)",
                    descriptor.record_id
                ),
            ));
        }
        let Some(durable) = descriptor.canonical_span else {
            return Err((
                false,
                format!(
                    "participant '{}' has no durable canonical span for its path in the \
                     indexed generation (pre-span lineage cannot be authenticated — denied)",
                    descriptor.record_id
                ),
            ));
        };
        if durable.start_line == 0
            || durable.end_line < durable.start_line
            || durable.end_line > line_count
        {
            return Err((
                false,
                format!(
                    "participant '{}' has a durable canonical span outside the retained \
                     revision",
                    descriptor.record_id
                ),
            ));
        }
        if (claimed_start, claimed_end) != (durable.start_line, durable.end_line) {
            return Err((
                false,
                format!(
                    "participant '{}' has a span that does not match the retained revision \
                     (its path or span was changed: claimed lines {claimed_start}-{claimed_end}, \
                     canonical lines {}-{})",
                    descriptor.record_id, durable.start_line, durable.end_line
                ),
            ));
        }
        // The authenticated payload span: the byte range of the DURABLE
        // span inside the digest-validated retained blob (r5 C4,
        // verification finding 1). Bounds were validated against
        // `line_count` above, so this cannot miss; if it ever does, the
        // participant DENIES rather than emitting an unauthenticated
        // payload.
        let Some(byte_span) = span_bytes(durable.start_line, durable.end_line) else {
            return Err((
                false,
                format!(
                    "participant '{}' has a durable canonical span that resolves to no \
                     byte range inside the retained revision",
                    descriptor.record_id
                ),
            ));
        };
        authenticated.push((
            descriptor,
            BoundParticipant {
                record_id: descriptor.record_id.clone(),
                start_line: durable.start_line,
                end_line: durable.end_line,
                byte_span,
            },
        ));
    }
    let Some(symbol) = symbol else {
        return Ok((
            None,
            authenticated
                .into_iter()
                .map(|(_, participant)| participant)
                .collect(),
        ));
    };
    // Symbol selection: names (mutable row state) select WHICH participant
    // is served, but the SERVED span is always the DURABLE canonical span —
    // repeated headers resolve to the record's canonical occurrence only.
    let mut spans: Vec<(u32, u32)> = authenticated
        .iter()
        .filter(|(descriptor, _)| descriptor.names.iter().any(|name| name == symbol))
        .map(|(_, participant)| (participant.start_line, participant.end_line))
        .collect();
    spans.sort();
    spans.dedup();
    let participants = authenticated
        .into_iter()
        .map(|(_, participant)| participant)
        .collect();
    match spans.as_slice() {
        [] => Err((
            true,
            format!("symbol '{symbol}' has no chunk in the indexed generation for this path"),
        )),
        [only] => {
            let Some((start, end)) = span_bytes(only.0, only.1) else {
                return Err((
                    true,
                    "the symbol's span is outside the retained blob bounds".to_owned(),
                ));
            };
            Ok((Some((start, end)), participants))
        }
        _ => Err((
            true,
            format!(
                "symbol '{symbol}' is ambiguous across {} chunks in the indexed generation",
                spans.len()
            ),
        )),
    }
}

fn binding_required() -> CoreError {
    CoreError::InvalidRequest {
        reason: "BindingRequired: no bound repository for exact source".into(),
    }
}

/// Denial reason for a bound path whose matched rows carry ONLY noncurrent
/// durable ownership: retired facts and non-published-generation lineage.
fn without_current_reason(lineage_record_ids: &[String]) -> String {
    format!(
        "bound source has no current canonical participants (retired or \
         non-published-generation durable lineage: {lineage_record_ids:?})"
    )
}

fn generation_denied(generation_id: &str) -> CoreError {
    CoreError::InvalidRequest {
        reason: format!(
            "ExactSourceUnavailable: generation {generation_id} is not published for this \
             repository and no authorized read lease covers it"
        ),
    }
}

impl CodeBindingCoordinator {
    /// Same-operation canonical-participant exact read (corrective r5, A4):
    /// participant resolution, authority, the retained descriptor, the byte
    /// read, the symbol span AND span-content validation all happen inside
    /// ONE consistent-read window over the durable records — a publication
    /// racing the operation retries once against the rebuilt view or denies
    /// (`GenerationChanged`); mixed-generation output is impossible by
    /// construction.
    ///
    /// Authority chain: participant identity is DURABLE LINEAGE matched
    /// against the published eligible bindings (never metadata stamps); the
    /// caller is authorized for the resolved origin repository inside the
    /// window; the retained descriptor's digest authenticates the bytes; and
    /// every participant's claimed span is validated against those exact
    /// bytes. Old-generation-only, RETIRED-fact and impostor rows DENY —
    /// bound code has no verbatim/stitching fallback (an explicitly
    /// authorized historical read goes through `exact_source` with a lease,
    /// a different operation).
    #[allow(clippy::double_must_use)]
    pub async fn exact_participant_source(
        &self,
        actor: &Actor,
        scope: &Scope,
        path: &str,
        candidates: &[ExactReadCandidate],
        symbol: Option<&str>,
        retained: &Arc<dyn RetainedSourceReader>,
    ) -> CoreResult<ParticipantExactSource> {
        let routing_view = self.eligibility_view(scope, None).await?;
        let anchor_repo = match route_against_view(&routing_view, path, candidates) {
            ParticipantRouting::Unbound => return Ok(ParticipantExactSource::Unbound),
            ParticipantRouting::WithoutCurrent { lineage_record_ids } => {
                return Ok(ParticipantExactSource::Denied {
                    symbol_selection: false,
                    reason: without_current_reason(&lineage_record_ids),
                });
            }
            ParticipantRouting::ForeignPath { record_ids } => {
                return Ok(ParticipantExactSource::Denied {
                    symbol_selection: false,
                    reason: foreign_path_reason(&record_ids),
                });
            }
            ParticipantRouting::Ambiguous { origins } => {
                return Ok(ParticipantExactSource::Denied {
                    symbol_selection: false,
                    reason: format!(
                        "matched chunks span multiple published repository generations: \
                         {origins:?}"
                    ),
                });
            }
            ParticipantRouting::Bound { repo_id, .. } => repo_id,
        };
        // The whole bound read runs inside ONE window: the closure re-routes
        // against the PINNED eligibility's view (a publication between the
        // routing read above and the pin is caught by the first attempt's
        // post-check; a publication DURING the read by the retry).
        let store = self.store();
        let authorizer = self.authorizer_handle();
        let path = path.to_owned();
        let candidates = candidates.to_vec();
        let symbol = symbol.map(str::to_owned);
        let closure_actor = actor.clone();
        let closure_scope = scope.clone();
        let retained = Arc::clone(retained);
        let anchor = RepoId(anchor_repo);
        self.consistent_read(
            actor,
            scope,
            &anchor,
            EXACT_READ_WINDOW,
            move |eligibility| {
                let store = Arc::clone(&store);
                let authorizer = Arc::clone(&authorizer);
                let path = path.clone();
                let candidates = candidates.clone();
                let symbol = symbol.clone();
                let actor = closure_actor.clone();
                let scope = closure_scope.clone();
                let retained = Arc::clone(&retained);
                async move {
                    // Re-resolve the participants INSIDE the pinned window —
                    // the authoritative participant set is this one, not the
                    // pre-window routing set.
                    let view = super::eligibility::durable_view_for_eligibility(
                        &store,
                        &scope,
                        &eligibility,
                    )
                    .await?;
                    let routed = route_against_view(&view, &path, &candidates);
                    let (repo_id, generation_id, descriptors) = match routed {
                        ParticipantRouting::Bound {
                            repo_id,
                            generation_id,
                            descriptors,
                        } => (repo_id, generation_id, descriptors),
                        ParticipantRouting::Unbound => {
                            return Ok(ParticipantExactSource::Unbound);
                        }
                        ParticipantRouting::WithoutCurrent { lineage_record_ids } => {
                            return Ok(ParticipantExactSource::Denied {
                                symbol_selection: false,
                                reason: without_current_reason(&lineage_record_ids),
                            });
                        }
                        ParticipantRouting::ForeignPath { record_ids } => {
                            return Ok(ParticipantExactSource::Denied {
                                symbol_selection: false,
                                reason: foreign_path_reason(&record_ids),
                            });
                        }
                        ParticipantRouting::Ambiguous { origins } => {
                            return Ok(ParticipantExactSource::Denied {
                                symbol_selection: false,
                                reason: format!(
                                    "matched chunks span multiple published repository \
                                     generations: {origins:?}"
                                ),
                            });
                        }
                    };
                    let repo = RepoId(repo_id.clone());
                    // Caller authority for the RESOLVED origin repository,
                    // validated inside the operation that serves the bytes.
                    authorizer
                        .authorize(&actor, &scope, &repo, CodeAuthorityAction::Inspect)
                        .await?;
                    let binding = store
                        .get_binding(&scope, &repo)
                        .await?
                        .ok_or_else(binding_required)?;
                    if binding.active_generation.as_deref() != Some(generation_id.as_str()) {
                        return Err(generation_denied(&generation_id));
                    }
                    if !eligibility
                        .generations
                        .iter()
                        .any(|id| id == &generation_id)
                    {
                        return Err(generation_denied(&generation_id));
                    }
                    let generation = store
                        .get_generation(&scope, &repo, &generation_id)
                        .await?
                        .ok_or_else(|| generation_denied(&generation_id))?;
                    // The retained descriptor selects the exact path/bytes.
                    let descriptor = store
                        .get_source_descriptor(&scope, &repo, &generation_id, &path)
                        .await?
                        .ok_or_else(|| CoreError::InvalidRequest {
                            reason: "ExactSourceUnavailable: no durable indexed-source \
                                     descriptor for the requested path"
                                .into(),
                        })?;
                    if descriptor.commit_oid != generation.commit_oid {
                        return Err(generation_denied(&generation_id));
                    }
                    let view = retained
                        .read_span(
                            &generation.commit_oid,
                            &descriptor.path,
                            None,
                            &descriptor.content_digest,
                        )
                        .map_err(|error| match error {
                            CoreError::InvalidRequest { reason }
                                if reason.contains("ExactSourceUnavailable") =>
                            {
                                CoreError::InvalidRequest { reason }
                            }
                            other => other,
                        })?;
                    // Revision-bound span + content validation over the
                    // digest-authenticated bytes, inside the same window.
                    // The authenticated participants (id + durable span +
                    // payload byte range) come from the SAME validated
                    // pass — a caller can never receive a participant id
                    // without its authenticated payload span (r5 C4,
                    // verification finding 1).
                    let (span, participants) =
                        match participant_span(&descriptors, symbol.as_deref(), &view.bytes) {
                            Ok(resolved) => resolved,
                            Err((symbol_selection, reason)) => {
                                return Ok(ParticipantExactSource::Denied {
                                    symbol_selection,
                                    reason,
                                });
                            }
                        };
                    Ok(ParticipantExactSource::Served {
                        repo_id,
                        generation_id,
                        span,
                        participants,
                        view,
                    })
                }
            },
        )
        .await
    }
    /// Serves the EXACT committed bytes of `path` for a bound repository.
    ///
    /// Authority chain (findings 2/3):
    /// 1. the CALLER is authorized (`Inspect`) for this scope + repository —
    ///    a denying authorizer denies even for fully populated scopes;
    /// 2. the binding must exist for THIS scope + repository;
    /// 3. `generation_id` (or the binding's active generation when `None`)
    ///    must be the PUBLISHED active generation, or be covered by a LIVE
    ///    read lease (`retained_generations` — the explicitly authorized
    ///    pinned historical read); staged, retired-without-lease, foreign and
    ///    unbound generations deny BEFORE any source byte is opened;
    /// 4. binding + generation + descriptor resolution runs inside a
    ///    consistent-read window (retry once on a publication race, else
    ///    `GenerationChanged`);
    /// 5. the ADAPTER reader validates custody and the durable descriptor
    ///    digest before bytes are declared exact (finding 7).
    #[allow(clippy::double_must_use)]
    pub async fn exact_source(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo_id: &str,
        generation_id: Option<&str>,
        historical_lease: Option<&CodeReadLease>,
        path: &str,
        span: Option<std::ops::Range<usize>>,
        retained: &Arc<dyn RetainedSourceReader>,
    ) -> CoreResult<ExactSourceView> {
        let repo = RepoId(repo_id.to_owned());
        // (1) Caller authority at the shared seam.
        self.authorize(actor, scope, &repo).await?;
        // (2)-(4) Resolve under a validated consistent-read window: the
        // closure observes the CURRENT published state through the lease it
        // pinned; a concurrent publication retries once or fails with
        // GenerationChanged rather than returning an unchecked view.
        let store = self.store();
        let repo_read = repo.clone();
        let historical_lease = historical_lease.cloned();
        self.consistent_read(actor, scope, &repo, EXACT_READ_WINDOW, move |eligibility| {
            let requested = generation_id.map(str::to_owned);
            let historical_lease = historical_lease.clone();
            let span = span.clone();
            let store = Arc::clone(&store);
            let repo = repo_read.clone();
            let retained = Arc::clone(retained);
            async move {
                let binding = store
                    .get_binding(scope, &repo)
                    .await?
                    .ok_or_else(binding_required)?;
                let active = binding.active_generation.clone();
                let resolved = match &requested {
                    Some(requested) => requested.clone(),
                    None => active.clone().ok_or_else(|| CoreError::InvalidRequest {
                        reason: "ExactSourceUnavailable: the bound repository has no \
                                     published generation"
                            .into(),
                    })?,
                };
                if Some(&resolved) != active.as_ref() {
                    // Historical authority is caller-presented and durable.
                    // Never authorize from the union of unrelated live leases:
                    // another caller's lease cannot turn a generation id into
                    // read authority.
                    let lease = historical_lease
                        .as_ref()
                        .ok_or_else(|| generation_denied(&resolved))?;
                    if !store
                        .validate_historical_read_lease(
                            lease,
                            scope,
                            repo_id,
                            &resolved,
                            chrono::Utc::now(),
                        )
                        .await?
                    {
                        return Err(generation_denied(&resolved));
                    }
                } else if !eligibility.generations.iter().any(|id| id == &resolved) {
                    // Defensive: the active generation must be in the pinned
                    // window's published set.
                    return Err(generation_denied(&resolved));
                }
                let generation = store
                    .get_generation(scope, &repo, &resolved)
                    .await?
                    .ok_or_else(|| generation_denied(&resolved))?;
                let descriptor = store
                    .get_source_descriptor(scope, &repo, &resolved, path)
                    .await?
                    .ok_or_else(|| CoreError::InvalidRequest {
                        reason: "ExactSourceUnavailable: no durable indexed-source \
                                 descriptor for the requested path"
                            .to_string(),
                    })?;
                if descriptor.commit_oid != generation.commit_oid {
                    return Err(generation_denied(&resolved));
                }
                // (5) Adapter-owned custody + digest-validated read.
                let view = retained
                    .read_span(
                        &generation.commit_oid,
                        path,
                        span,
                        &descriptor.content_digest,
                    )
                    .map_err(|error| match error {
                        CoreError::InvalidRequest { reason }
                            if reason.contains("ExactSourceUnavailable") =>
                        {
                            CoreError::InvalidRequest { reason }
                        }
                        other => other,
                    })?;
                // The read window extends PAST the byte I/O: a historical
                // lease released or expired DURING the read denies the
                // answer — the token must still be live when the bytes are
                // handed back (release/expiry-during-read barrier).
                if let Some(lease) = historical_lease.as_ref()
                    && !store
                        .validate_historical_read_lease(
                            lease,
                            scope,
                            repo_id,
                            &resolved,
                            chrono::Utc::now(),
                        )
                        .await?
                {
                    return Err(generation_denied(&resolved));
                }
                Ok(view)
            }
        })
        .await
    }
}

impl crate::EngramProvider {
    /// Serves the EXACT committed bytes of `path` for a bound repository's
    /// published generation through the provider's authorized retained-source
    /// handle (see [`CodeBindingCoordinator::exact_source`]).
    #[allow(clippy::double_must_use)]
    pub async fn code_fetch_exact(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo_id: &str,
        path: &str,
        span: Option<std::ops::Range<usize>>,
    ) -> CoreResult<ExactSourceView> {
        let coordinator = self.code_bindings()?;
        let retained = self.retained_code_source()?;
        coordinator
            .exact_source(actor, scope, repo_id, None, None, path, span, retained)
            .await
    }

    /// [`Self::code_fetch_exact`] pinned to one indexed generation — the
    /// revision a specific chunk set was stamped with. The generation must be
    /// the published active generation or live-lease-covered; bytes still
    /// come from the retained snapshot of THAT commit, never from a checkout.
    #[allow(clippy::double_must_use)]
    pub async fn code_fetch_exact_at(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo_id: &str,
        generation_id: &str,
        path: &str,
        span: Option<std::ops::Range<usize>>,
    ) -> CoreResult<ExactSourceView> {
        self.code_fetch_exact_at_with_lease(actor, scope, repo_id, generation_id, None, path, span)
            .await
    }

    /// Fetches a historical generation only when this caller presents the
    /// durable canonical lease that pins it.  The active generation needs no
    /// historical token, while a generation id alone is deliberately denied.
    #[allow(clippy::double_must_use)]
    pub async fn code_fetch_exact_at_with_lease(
        &self,
        actor: &Actor,
        scope: &Scope,
        repo_id: &str,
        generation_id: &str,
        historical_lease: Option<&CodeReadLease>,
        path: &str,
        span: Option<std::ops::Range<usize>>,
    ) -> CoreResult<ExactSourceView> {
        let coordinator = self.code_bindings()?;
        let retained = self.retained_code_source()?;
        coordinator
            .exact_source(
                actor,
                scope,
                repo_id,
                Some(generation_id),
                historical_lease,
                path,
                span,
                retained,
            )
            .await
    }
}

impl crate::EngramProvider {
    /// Same-operation canonical-participant exact read for one indexed path
    /// (corrective r5, A4): the provider composes its knowledge listing
    /// snapshots (`candidates`), its binding coordinator and its retained
    /// reader into ONE authorized operation — see
    /// [`CodeBindingCoordinator::exact_participant_source`]. Providers
    /// without a binding coordinator report `Unbound` (the caller's ordinary
    /// paths apply).
    #[allow(clippy::double_must_use)]
    pub async fn code_fetch_exact_participants(
        &self,
        actor: &Actor,
        scope: &Scope,
        path: &str,
        candidates: &[ExactReadCandidate],
        symbol: Option<&str>,
    ) -> CoreResult<ParticipantExactSource> {
        let Ok(coordinator) = self.code_bindings() else {
            return Ok(ParticipantExactSource::Unbound);
        };
        let Ok(retained) = self.retained_code_source() else {
            return Ok(ParticipantExactSource::Unbound);
        };
        coordinator
            .exact_participant_source(actor, scope, path, candidates, symbol, retained)
            .await
    }
}
