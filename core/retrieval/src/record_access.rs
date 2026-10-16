//! Request-bound record selection; graph caches remain principal-independent.
use engram_domain::{Policy, RetrievalTargetType, Visibility};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct RecordAccess {
    restricted: HashSet<String>,
    permitted: HashMap<String, Policy>,
}
fn key(kind: &RetrievalTargetType, id: &str) -> String {
    format!("{kind:?}:{id}")
}
impl RecordAccess {
    pub fn restrict(&mut self, kind: RetrievalTargetType, id: &str) {
        self.restricted.insert(key(&kind, id));
    }
    /// Any independent authorized owner may share the identical physical record.
    /// Preserve a whole authorized owner's policy, preferring private visibility;
    /// never construct a permissive synthetic policy for a source-owned record.
    pub fn permit(&mut self, kind: RetrievalTargetType, id: &str, policy: Policy) {
        let key = key(&kind, id);
        let rank = |p: &Policy| match p.visibility {
            Visibility::Private => 0,
            Visibility::Workspace => 1,
            Visibility::Organization => 2,
            Visibility::Public => 3,
        };
        match self.permitted.entry(key) {
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(policy);
            }
            std::collections::hash_map::Entry::Occupied(mut o) => {
                if rank(&policy) < rank(o.get()) {
                    o.insert(policy);
                }
            }
        }
    }
    pub fn allows(&self, kind: RetrievalTargetType, id: &str) -> bool {
        let key = key(&kind, id);
        !self.restricted.contains(&key) || self.permitted.contains_key(&key)
    }
    pub fn policy(&self, kind: RetrievalTargetType, id: &str) -> Option<&Policy> {
        self.permitted.get(&key(&kind, id))
    }
}
