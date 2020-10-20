pub struct MatchTo(pub usize, pub Vec<Target>);
pub struct Target(pub Option<usize>, pub u32);

pub struct Matching(Vec<Match>);
impl std::iter::IntoIterator for Matching {
    type Item = Match;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
pub struct Match(pub usize, pub Option<usize>);
pub fn greedy_match(mut candidates: Vec<MatchTo>, track_count: usize) -> Matching {
    // greedy match:
    // pick candidate with least cost match
    // fix it to that match
    // repeat until done
    let mut used_old: Vec<bool> = vec![false; track_count];
    let mut matching = Matching(Vec::with_capacity(candidates.len()));
    candidates
        .iter_mut()
        .for_each(|MatchTo(_, opts)| opts.sort_unstable_by_key(|tup| tup.1));
    candidates.sort_unstable_by_key(|MatchTo(_, opts)| opts.len());
    for MatchTo(new, opts) in candidates.into_iter() {
        let Target(maybe_oldi, _cost) = opts
            .into_iter()
            .find(|Target(maybe_oldi, _cost)| match maybe_oldi {
                Some(oldi) => !used_old[*oldi],
                None => true,
            })
            .expect("Conflict!  Shouldn't be possible!");
        match maybe_oldi {
            Some(oldi) => {
                used_old[oldi] = true;
                matching.0.push(Match(new, Some(oldi)));
            }
            None => {
                matching.0.push(Match(new, None));
            }
        }
    }
    matching
}
