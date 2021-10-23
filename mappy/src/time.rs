use crossbeam::channel::{bounded, Receiver, Sender};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub trait TimerID: Copy + PartialEq + Eq + Ord + std::fmt::Debug + Send + 'static {}

#[derive(Clone, Copy, Debug)]
pub enum TimerQuery<T: TimerID> {
    Percentile(T, u8),
    Mean(T),
    StdDev(T),
    Count(T),
    Max(T),
    Min(T),
    Sum(T),
}
enum TimerMessage<T: TimerID> {
    Record(T, Duration),
    List,
    Query(Vec<TimerQuery<T>>),
    Terminate,
}
#[derive(Debug)]
enum TimerResponse<T: TimerID> {
    List(Vec<T>),
    Queries(Vec<f64>),
}
pub struct Timers<T: TimerID> {
    tx: Arc<Sender<TimerMessage<T>>>,
    rx: Receiver<TimerResponse<T>>,
}
pub struct ReadyTimer<T: TimerID>(T, Arc<Sender<TimerMessage<T>>>);
pub struct RunningTimer<T: TimerID>(T, Instant, Arc<Sender<TimerMessage<T>>>);

impl<T: TimerID> Timers<T> {
    pub fn new() -> Self {
        let (tx, t_rx) = bounded(1);
        let (t_tx, rx) = bounded(1);
        rayon::spawn(move || {
            let mut rec = BTreeMap::new();
            for tm in t_rx.iter() {
                match tm {
                    TimerMessage::Record(t, dur) => rec.entry(t).or_insert(vec![]).push(dur),
                    TimerMessage::Terminate => break,
                    TimerMessage::List => t_tx
                        .send(TimerResponse::List(rec.keys().copied().collect()))
                        .unwrap(),
                    TimerMessage::Query(qs) => t_tx
                        .send(TimerResponse::Queries(Self::handle_queries(&rec, qs)))
                        .unwrap(),
                }
            }
        });
        Timers {
            tx: Arc::new(tx),
            rx,
        }
    }
    pub fn timer(&self, t: T) -> ReadyTimer<T> {
        ReadyTimer(t, Arc::clone(&self.tx))
    }
    fn handle_queries(rec: &BTreeMap<T, Vec<Duration>>, qs: Vec<TimerQuery<T>>) -> Vec<f64> {
        qs.into_iter()
            .map(|qi| match qi {
                TimerQuery::Percentile(t, p) => {
                    assert!(p <= 100);
                    let mut all = rec[&t].clone();
                    all.sort();
                    let scaled_p = (p as f64 / 100.0) * (all.len() - 1) as f64;
                    // find the threshold under which p% of the data lie
                    all[scaled_p as usize].as_secs_f64()
                }
                TimerQuery::Mean(t) => {
                    rec[&t].iter().sum::<Duration>().as_secs_f64() / rec[&t].len() as f64
                }
                TimerQuery::StdDev(t) => {
                    let mean =
                        rec[&t].iter().sum::<Duration>().as_secs_f64() / rec[&t].len() as f64;
                    (rec[&t]
                        .iter()
                        .map(|xi| (xi.as_secs_f64() - mean) * (xi.as_secs_f64() - mean))
                        .sum::<f64>()
                        / (rec[&t].len() - 1) as f64)
                        .sqrt()
                }
                TimerQuery::Sum(t) => rec[&t].iter().sum::<Duration>().as_secs_f64(),
                TimerQuery::Count(t) => rec[&t].len() as f64, // suspicious
                TimerQuery::Max(t) => rec[&t]
                    .iter()
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap()
                    .as_secs_f64(),
                TimerQuery::Min(t) => rec[&t]
                    .iter()
                    .min_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap()
                    .as_secs_f64(),
            })
            .collect()
    }
    pub fn timers(&self) -> Vec<T> {
        self.tx.send(TimerMessage::List).unwrap();
        match self.rx.recv().unwrap() {
            TimerResponse::List(ts) => ts,
            m => panic!("Received wrong response for list message! {:?}", m),
        }
    }
    pub fn queries(&self, qs: Vec<TimerQuery<T>>) -> Vec<f64> {
        self.tx.send(TimerMessage::Query(qs)).unwrap();
        match self.rx.recv().unwrap() {
            TimerResponse::Queries(vs) => vs,
            m => panic!("Received wrong response for queries message! {:?}", m),
        }
    }
    pub fn query(&self, q: TimerQuery<T>) -> f64 {
        self.queries(vec![q])[0]
    }
    pub fn max(&self, t: T) -> f64 {
        self.query(TimerQuery::Max(t))
    }
    pub fn min(&self, t: T) -> f64 {
        self.query(TimerQuery::Min(t))
    }
    pub fn percentile(&self, t: T, p: u8) -> f64 {
        self.query(TimerQuery::Percentile(t, p))
    }
    pub fn mean(&self, t: T) -> f64 {
        self.query(TimerQuery::Mean(t))
    }
    pub fn stddev(&self, t: T) -> f64 {
        self.query(TimerQuery::StdDev(t))
    }
    pub fn count(&self, t: T) -> usize {
        self.query(TimerQuery::Count(t)) as usize
    }
    pub fn sum(&self, t: T) -> f64 {
        self.query(TimerQuery::Sum(t))
    }
}
impl<T: TimerID> std::fmt::Display for Timers<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for t in self.timers() {
            //count, min, mean, 95p, max
            let stats = self.queries(vec![
                TimerQuery::Count(t),
                TimerQuery::Mean(t),
                TimerQuery::Percentile(t, 95),
                TimerQuery::Percentile(t, 99),
                TimerQuery::Max(t),
                TimerQuery::Sum(t),
            ]);
            writeln!(
                f,
                "{:?}({}) mean {} -- p95 {} -- p99 {} -- max {} :: net {}",
                t,
                stats[0],
                stats[1] * 1000.0,
                stats[2] * 1000.0,
                stats[3] * 1000.0,
                stats[4] * 1000.0,
                stats[5] * 1000.0
            )?;
        }
        Ok(())
    }
}
impl<T: TimerID> Drop for Timers<T> {
    fn drop(&mut self) {
        self.tx.send(TimerMessage::Terminate).unwrap();
    }
}
impl<T: TimerID> ReadyTimer<T> {
    pub fn start(self) -> RunningTimer<T> {
        RunningTimer(self.0, Instant::now(), self.1)
    }
}
impl<T: TimerID> RunningTimer<T> {
    pub fn stop(self) {
        self.2
            .send(TimerMessage::Record(self.0, self.1.elapsed()))
            .unwrap();
    }
}
