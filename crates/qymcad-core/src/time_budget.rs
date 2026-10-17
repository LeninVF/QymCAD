//! THE BUDGETS OF TIME OF THE PROBES. A probe of time measures the machine as much as the program: run among the other
//! checks of a crate it reads their load - a budget of 3 s read over it on a sketch of 1 200 segments that takes 1.66 s
//! alone, three runs of nine. So a budget holds at the level of the probes of time alone (`QYMCAD_TIER=time`,
//! `tools/gate.py time`), where they run one at a time, and is scaled there by `QYMCAD_TIME_SCALE` (CI sets 2). What a
//! probe of time checks besides the time - that the work was done, and done right - holds at every level.
//!
//! THE COST OF THE WORK, NOT THE CLOCK ON THE WALL. A machine shared with other work gives a probe a share of its
//! processors, and the wall clock of the probe stretches with the share: under a load of 40-50 on 20 threads a click
//! meant to land 300 ms into a rebuild landed 266-435 ms late, a rebuild of 13 s took 30.4 s. What the program spent is
//! read from the kernel of the system instead (`Cost`): the time a thread ran or was blocked on what the program waits
//! for, less the time it stood ready with no processor free; or the processor time of the whole process, when the work
//! runs on threads of its own and the probe is alone in its process. On a system without these counters (they are
//! read from `/proc`) the cost is the wall clock, as it was.

use std::time::{Duration, Instant};

/// Whether this run is the level of the probes of time.
pub fn at_the_level_of_time() -> bool {
    std::env::var("QYMCAD_TIER").as_deref() == Ok("time")
}

/// The budget on this machine: the budget times `QYMCAD_TIME_SCALE`, 1 when unset. A budget past what a `Duration`
/// holds - `Duration::MAX`, a step with no budget - stays no budget rather than overflowing the multiplication.
pub fn scaled(budget: Duration) -> Duration {
    let k = std::env::var("QYMCAD_TIME_SCALE").ok().and_then(|v| v.parse::<f64>().ok()).filter(|k| *k >= 1.0).unwrap_or(1.0);
    Duration::try_from_secs_f64(budget.as_secs_f64() * k).unwrap_or(Duration::MAX)
}

/// Whether `took` keeps within `budget`: always, but at the level of the probes of time, against the scaled budget.
pub fn within(took: Duration, budget: Duration) -> bool {
    !at_the_level_of_time() || took <= scaled(budget)
}

/// WHAT A RED OF TIME SAYS FIRST, read by `tools/gate.py`: a work over its budget, and a wait for an event that did not
/// come. The gate names such a red with the load the machine was under.
pub const SAID_OVER_BUDGET: &str = "over its budget of time";
pub const SAID_NOT_IN_TIME: &str = "did not happen in time";

/// WHAT DOES THE WORK THAT IS MEASURED.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doer {
    /// The thread that measures, by itself: the cost is its time less the time it stood ready with no processor.
    Thread,
    /// The whole process, the work on threads of its own: the cost is the processor time of every thread. True only
    /// when the probe is alone in its process - a probe of the acceptance set, a check run one at a time.
    Process,
}

/// THE COST OF A PIECE OF WORK.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cost {
    /// What the budget is held against: for a thread its time on the wall less `waited` - running, or blocked on what
    /// the program waits for, so a frame stalled on a lock is a slow frame - for a process `cpu`.
    pub work: Duration,
    /// The clock on the wall, from start to end.
    pub wall: Duration,
    /// The time on a processor: of the thread - read to a tick of the scheduler, a few milliseconds, as the system
    /// counts a running thread's time at its ticks - or of every thread of the process, to 10 ms.
    pub cpu: Duration,
    /// How long the threads stood ready to run with no processor free for them: the share the machine gave to others.
    pub waited: Duration,
}

impl std::fmt::Display for Cost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.3?} of work ({:.3?} on the wall: {:.3?} on a processor, {:.3?} waiting for one)", self.work, self.wall, self.cpu, self.waited)
    }
}

/// A WORK AND WHAT IT COST.
pub struct Timed<T> {
    pub out: T,
    pub cost: Cost,
}

/// THE COUNTERS OF THE SYSTEM AT ONE INSTANT: the clock on the wall, the time run and waited by the threads, and the
/// processor time of the whole process; `counted` is false where the system gives no counters.
struct Reading {
    at: Instant,
    threads: Vec<ThreadTime>,
    process: Duration,
    counted: bool,
}

/// The time one thread ran on a processor and stood ready waiting for one, by its id (empty for the measuring thread).
struct ThreadTime {
    id: String,
    ran: Duration,
    waited: Duration,
}

/// The time a thread ran and waited, from its `schedstat`: nanoseconds on a processor, nanoseconds ready on a queue.
fn thread_time(id: String, schedstat: &str) -> Option<ThreadTime> {
    let text = std::fs::read_to_string(schedstat).ok()?;
    let mut f = text.split_whitespace().map(|v| v.parse::<u64>().ok());
    let ran = f.next()??;
    let waited = f.next()??;
    Some(ThreadTime { id, ran: Duration::from_nanos(ran), waited: Duration::from_nanos(waited) })
}

/// The processor time of the whole process, from `/proc/self/stat`: user and system, in ticks of 1/100 s (the tick of
/// that file is fixed by the interface of the system, not by the machine).
fn process_time() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let fields: Vec<&str> = stat.rsplit(')').next()?.split_whitespace().collect();
    let ticks = fields.get(11)?.parse::<u64>().ok()? + fields.get(12)?.parse::<u64>().ok()?;
    Some(Duration::from_millis(ticks * 10))
}

/// THE COUNTERS AND THE CLOCK, read in the order `when` says. A thread taken off its processor between the two leaves
/// that wait in the counters and not in the clock, or the other way round; the order is chosen so the wait is never
/// charged as work - at the start the counters first, at the end the clock first. Read the other way, 355 frames of 0.80 s
/// among spinning threads were charged 0.20 s while the whole of them did 0.08 s of work.
fn read(doer: Doer, when: Moment) -> Reading {
    let clock_first = Instant::now();
    let threads: Vec<ThreadTime> = match doer {
        Doer::Thread => thread_time(String::new(), "/proc/thread-self/schedstat").into_iter().collect(),
        Doer::Process => std::fs::read_dir("/proc/self/task")
            .map(|d| d.flatten().filter_map(|e| thread_time(e.file_name().to_string_lossy().into_owned(), &format!("{}/schedstat", e.path().display()))).collect())
            .unwrap_or_default(),
    };
    let process = process_time();
    let at = match when {
        Moment::Start => Instant::now(),
        Moment::End => clock_first,
    };
    Reading { at, counted: !threads.is_empty() && process.is_some(), threads, process: process.unwrap_or_default() }
}

/// Which end of a measure a reading is.
#[derive(Clone, Copy)]
enum Moment {
    Start,
    End,
}

/// A MEASURE STARTED: the cost of whatever runs from `start` to `cost`.
pub struct Meter {
    doer: Doer,
    from: Reading,
}

impl Meter {
    pub fn start(doer: Doer) -> Self {
        Self { doer, from: read(doer, Moment::Start) }
    }

    /// The cost from the start to now.
    pub fn cost(&self) -> Cost {
        let to = read(self.doer, Moment::End);
        let wall = to.at.duration_since(self.from.at);
        if !self.from.counted || !to.counted {
            return Cost { work: wall, wall, cpu: wall, waited: Duration::ZERO };
        }
        // a thread started during the work counts from nothing; one that ended took its waiting with it
        let before = |id: &str| self.from.threads.iter().find(|t| t.id == id);
        let since = |t: &ThreadTime, of: fn(&ThreadTime) -> Duration| of(t).saturating_sub(before(&t.id).map_or(Duration::ZERO, of));
        let waited = to.threads.iter().map(|t| since(t, |t| t.waited)).sum();
        match self.doer {
            Doer::Thread => {
                let cpu = to.threads.iter().map(|t| since(t, |t| t.ran)).sum();
                Cost { work: wall.saturating_sub(waited), wall, cpu, waited }
            }
            Doer::Process => {
                let cpu = to.process.saturating_sub(self.from.process);
                Cost { work: cpu, wall, cpu, waited }
            }
        }
    }
}

/// THE COST OF `work`, done by `doer`.
pub fn cost_of<T>(doer: Doer, work: impl FnOnce() -> T) -> Timed<T> {
    let meter = Meter::start(doer);
    let out = work();
    Timed { out, cost: meter.cost() }
}

/// HOW BUSY THE MACHINE IS: the load of the last minute against the threads it has.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Machine {
    pub load: f64,
    pub threads: usize,
}

impl Machine {
    pub fn now() -> Self {
        let load = std::fs::read_to_string("/proc/loadavg").ok().and_then(|l| l.split_whitespace().next().and_then(|v| v.parse().ok())).unwrap_or(0.0);
        Self { load, threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) }
    }
}

impl std::fmt::Display for Machine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the machine under a load of {:.1} on {} threads", self.load, self.threads)
    }
}

/// THE WORDS OF A WORK OVER ITS BUDGET: what, its cost, the budget, and how busy the machine was.
pub fn over(what: &str, cost: Cost, budget: Duration) -> String {
    format!("{what} {SAID_OVER_BUDGET}: {cost}, budget {budget:.3?}, {}", Machine::now())
}

/// Whether `cost` keeps within `budget` scaled for this machine, at every level.
pub fn holds(cost: Cost, budget: Duration) -> bool {
    cost.work <= scaled(budget)
}

#[cfg(test)]
mod tests {
    use super::{cost_of, Doer};
    use std::time::{Duration, Instant};

    /// A STEP WITH NO BUDGET (`Duration::MAX`) stays with none when scaled, rather than overflowing: a probe of time that
    /// gave a step no budget panicked inside the multiplication.
    #[test]
    fn no_budget_stays_no_budget() {
        assert_eq!(super::scaled(Duration::MAX), Duration::MAX);
        assert!(super::within(Duration::from_secs(3600), Duration::MAX));
    }

    /// Spin for `span` of the wall: work for a processor and nothing else.
    fn spin(span: Duration) -> u64 {
        let until = Instant::now() + span;
        let mut n = 0u64;
        while Instant::now() < until {
            n = n.wrapping_add(1);
        }
        n
    }

    /// A THREAD KEPT WAITING FOR A PROCESSOR IS NOT CHARGED FOR THE WAIT: with four spinning threads a processor, the
    /// work of a thread is a part of its wall, and the rest is what it waited. Measured on 20 threads, three runs: 0.40 s
    /// of the wall, 0.06-0.11 s of work, 0.29-0.34 s waited.
    #[test]
    fn the_wait_for_a_processor_is_not_work() {
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        let crowd: Vec<_> = (0..4 * threads).map(|_| std::thread::spawn(|| spin(Duration::from_millis(600)))).collect();
        let timed = cost_of(Doer::Thread, || spin(Duration::from_millis(400)));
        crowd.into_iter().for_each(|t| drop(t.join()));
        let cost = timed.cost;
        eprintln!("a thread among {} spinning ones: {cost}", 4 * threads);
        if !super::read(Doer::Thread, super::Moment::End).counted {
            eprintln!("PASSED OVER: this system gives no counters of a thread's time; the cost is the wall clock");
            return;
        }
        assert!(cost.waited > Duration::ZERO && cost.work < cost.wall.mul_f64(0.75), "a thread among {} spinning ones: {cost}", 4 * threads);
        assert!(cost.work <= cost.cpu + Duration::from_millis(20), "spinning is all on a processor: {cost}");
    }

    /// A THREAD BLOCKED ON WHAT THE PROGRAM WAITS FOR IS CHARGED FOR IT: a frame stalled on a lock is a slow frame.
    #[test]
    fn a_block_is_work() {
        let cost = cost_of(Doer::Thread, || std::thread::sleep(Duration::from_millis(80))).cost;
        assert!(cost.work >= Duration::from_millis(80), "a sleep of 80 ms: {cost}");
    }

    /// THE WORK OF A PROCESS COUNTS ITS OTHER THREADS: a thread that waits for a worker spends nothing on a processor,
    /// and the worker's 300 ms are counted.
    #[test]
    fn a_process_counts_its_workers() {
        let cost = cost_of(Doer::Process, || std::thread::spawn(|| spin(Duration::from_millis(300))).join()).cost;
        if !super::read(Doer::Process, super::Moment::End).counted {
            eprintln!("PASSED OVER: this system gives no counters of a process's time; the cost is the wall clock");
            return;
        }
        assert!(cost.work >= Duration::from_millis(250), "a worker spinning 300 ms: {cost}");
    }
}
