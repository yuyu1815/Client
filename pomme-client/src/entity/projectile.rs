//! Collision-free, display-only projectile prediction. Packet positions remain
//! authoritative.
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};

use glam::DVec3;

use super::components::Position;

pub(super) const HORIZON: usize = 64;
pub(super) const THRESHOLD: usize = 16;
// ponytail: cap each job at 512 entities (~2.1 MiB of frames), one in flight;
// overflow takes the identical synchronous path. Profile before raising the
// cap.
pub(super) const MAX_BATCH: usize = 512;

#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    pub position: Position,
    pub velocity: DVec3,
}

pub(super) struct Input {
    pub id: i32,
    pub revision: u64,
    pub start: u64,
    pub source: Frame,
    pub arrow: bool,
    pub no_gravity: bool,
    pub drag: f64,
}

#[derive(Clone, Debug)]
pub(super) struct Flight {
    pub id: i32,
    pub revision: u64,
    pub start: u64,
    pub drag: f64,
    pub frames: Box<[Frame; HORIZON + 1]>,
}

pub(super) struct Job {
    pub epoch: u64,
    pub inputs: Vec<Input>,
}

pub(super) struct Result {
    pub epoch: u64,
    pub flights: Vec<Flight>,
}

pub(super) struct Worker {
    send: SyncSender<Job>,
    recv: Receiver<Result>,
    pub in_flight: bool,
}

impl Worker {
    #[cfg(test)]
    pub(super) fn held() -> (Self, Receiver<Job>, SyncSender<Result>) {
        let (send, jobs) = mpsc::sync_channel(1);
        let (results, recv) = mpsc::sync_channel(1);
        (
            Self {
                send,
                recv,
                in_flight: true,
            },
            jobs,
            results,
        )
    }

    pub fn new() -> Option<Self> {
        let (send, jobs) = mpsc::sync_channel::<Job>(1);
        let (results, recv) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("projectile-trajectories".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let result = Result {
                        epoch: job.epoch,
                        flights: job.inputs.into_iter().map(flight).collect(),
                    };
                    if results.send(result).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self {
            send,
            recv,
            in_flight: false,
        })
    }

    pub fn poll(&mut self) -> std::result::Result<Option<Result>, TryRecvError> {
        match self.recv.try_recv() {
            Ok(result) => {
                self.in_flight = false;
                Ok(Some(result))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn submit(&mut self, job: Job) -> bool {
        if self.in_flight {
            return false;
        }
        if self.send.try_send(job).is_err() {
            return false;
        }
        self.in_flight = true;
        true
    }
}

/// Vanilla 26.2 ordering: arrow move -> drag -> gravity; snowball gravity
/// -> inertia -> move. Same function runs on the main thread on cache misses.
pub(super) fn step(frame: Frame, arrow: bool, no_gravity: bool, drag: f64) -> Frame {
    let gravity = if no_gravity {
        0.0
    } else if arrow {
        0.05
    } else {
        0.03
    };
    let velocity = if arrow {
        frame.velocity * drag - DVec3::new(0.0, gravity, 0.0)
    } else {
        (frame.velocity - DVec3::new(0.0, gravity, 0.0)) * drag
    };
    Frame {
        position: frame.position + if arrow { frame.velocity } else { velocity },
        velocity,
    }
}

pub(super) fn flight(input: Input) -> Flight {
    let mut frames = Box::new([input.source; HORIZON + 1]);
    for i in 1..=HORIZON {
        let next = step(frames[i - 1], input.arrow, input.no_gravity, input.drag);
        frames[i] = if next.position.is_finite() && next.velocity.is_finite() {
            next
        } else {
            frames[i - 1] // invalid display math holds; server corrections still apply
        };
    }
    Flight {
        id: input.id,
        revision: input.revision,
        start: input.start,
        drag: input.drag,
        frames,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_worker_computes_finite_bounded_owned_flight() {
        let mut worker = Worker::new().expect("worker thread");
        let job = Job {
            epoch: 77,
            inputs: vec![Input {
                id: 5,
                revision: 9,
                start: 100,
                source: Frame {
                    position: Position::new(2.0, 70.0, 2.0),
                    velocity: DVec3::X,
                },
                arrow: true,
                no_gravity: true,
                drag: 0.99_f32 as f64,
            }],
        };
        assert!(worker.submit(job));
        assert!(!worker.submit(Job {
            epoch: 88,
            inputs: vec![]
        }));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let result = loop {
            if let Some(result) = worker.poll().unwrap() {
                break result;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "worker did not finish"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert_eq!(result.epoch, 77);
        assert_eq!(result.flights.len(), 1);
        assert_eq!(result.flights[0].start, 100);
        assert_eq!(result.flights[0].revision, 9);
        assert!((result.flights[0].frames[2].position.x - (3.0 + 0.99_f32 as f64)).abs() < 1e-12);
        assert_eq!(result.flights[0].frames[HORIZON].position.y, 70.0);
    }

    #[test]
    fn vanilla_order_and_absolute_horizon() {
        let input = Input {
            id: 1,
            revision: 0,
            start: 15,
            source: Frame {
                position: Position::new(2.0, 70.0, 2.0),
                velocity: DVec3::X,
            },
            arrow: true,
            no_gravity: false,
            drag: 0.99_f32 as f64,
        };
        let frames = flight(input).frames;
        assert_eq!(frames[1].position.x, 3.0);
        assert!((frames[2].position.x - (3.0 + 0.99_f32 as f64)).abs() < 1e-12);
        assert_eq!(frames[1].velocity.y, -0.05);
        assert!(frames[64].position.x > frames[2].position.x);
        let snow = step(frames[0], false, false, 0.99_f32 as f64);
        assert!((snow.position.x - 2.9900000095367432).abs() < 1e-12);
        assert!((snow.position.y - 69.9702999997139).abs() < 1e-12);
        assert_eq!(
            step(frames[0], false, true, 0.99_f32 as f64).position.y,
            70.0
        );
    }
}
