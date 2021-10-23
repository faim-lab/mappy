#[derive(Debug)]
pub struct MatchTo(pub usize, pub Vec<Target>);
#[derive(Debug)]
pub struct Target(pub Option<usize>, pub u32);

#[derive(Debug)]
pub struct Matching(Vec<Match>);
impl std::iter::IntoIterator for Matching {
    type Item = Match;
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
#[derive(Debug)]
pub struct Match(pub usize, pub Option<usize>);
#[allow(unused)]
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

pub fn bnb_match(mut candidates: Vec<MatchTo>, track_count: usize) -> Matching {
    // bnb match:
    // sort each candidate's options
    // track which options have been used
    // keep the best whole matching seen so far
    let mut used_old: Vec<bool> = vec![false; track_count];
    candidates
        .iter_mut()
        .for_each(|MatchTo(_, opts)| opts.sort_unstable_by_key(|tup| tup.1));
    candidates.sort_unstable_by_key(|MatchTo(_, opts)| opts.len());
    // for each candidate, pick the best unmatched option that wouldn't increase the net score beyond the bound.
    let mut current = 0;
    let mut cost = 0;
    let mut picks = Vec::with_capacity(candidates.len());
    let mut bound = 30 * candidates.len() as u32;
    let mut best = vec![];

    // println!("start");
    let mut _tries = 0;
    loop {
        // println!("try {:?} current {:?} bound {:?}", tries, current, bound);
        assert!(current <= picks.len());
        let start_pick = if current == picks.len() {
            picks.push(0);
            0
        } else {
            let options = &candidates[current].1;
            let old_pick = picks[current];
            // println!("unpick {:?}", old_pick);
            cost -= options[old_pick].1;
            if let Some(old) = options[old_pick].0 {
                used_old[old] = false;
            }
            old_pick+1
        };
        let options = &candidates[current].1;
        if let Some(pick) = options.iter().skip(start_pick).position(|Target(maybe_oldi,ncost)| maybe_oldi.map(|oi| !used_old[oi]).unwrap_or(true) && cost+ncost < bound) {
            let pick = start_pick + pick;
            // println!("{:?} / {:?} --  {:?} + {:?}", pick, options.len(), cost, options[pick].1);
            picks[current] = pick;
            // increase cost
            cost += options[pick].1;
            // mark used
            if let Some(oi) = options[pick].0 {
                used_old[oi] = true;
            }
            //if we're here we know the cost is less than bound
            if picks.len() == candidates.len() {
                // this is a complete assignment, update bound
                // we know cost < bound at this point!
                bound = cost;
                best = picks.clone();
                // then continue, since this was the best we had
                // println!("found cost {:?}", bound);
            } else {
                current += 1;
            }
        } else {
            // println!("backtrack");
            //no better options, backtrack
            picks.pop();
            if current > 0 {
                current -= 1;
            } else {
                break;
            }
        }
        _tries += 1;
    }
    // dbg!(&best,bound);
    Matching(
        candidates.into_iter().enumerate().map(|(which, MatchTo(new,opts))| {
            Match(new,opts[best[which]].0)
        }).collect()
    )
}
