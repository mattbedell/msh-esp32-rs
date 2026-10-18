use defmt;
use thiserror::Error;

pub const FRAME_START1: u8 = 0x94;
pub const FRAME_START2: u8 = 0xc3;
pub const FRAME_MAX_SIZE: usize = 512; // frame bytes included

pub trait ConsumedFrameBytes {
    fn consumed(&self) -> usize;
}

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum State {
    Search,
    FoundS1,
    FoundS2,
    FoundMSB(u8),
    Msg {
        len: usize,
        remaining: usize,
        head: usize,
    },
}

pub enum Frame {
    Complete { len: usize, consumed: usize },
    Incomplete { consumed: usize },
}

impl ConsumedFrameBytes for Frame {
    fn consumed(&self) -> usize {
        match self {
            Self::Complete { consumed, .. } | Self::Incomplete { consumed } => *consumed,
        }
    }
}

#[derive(Debug, Error)]
#[error("{kind} (consumed {consumed} bytes)")]
pub struct FrameProcessorError {
    consumed: usize,
    pub kind: FrameErrorKind,
}

impl ConsumedFrameBytes for FrameProcessorError {
    fn consumed(&self) -> usize {
        self.consumed
    }
}

#[derive(Clone, Copy, Debug, Error)]
#[non_exhaustive]
pub enum FrameErrorKind {
    #[error("frame length {frame_len} exceeds buffer capacity {capacity}")]
    InsufficientCapacity { capacity: usize, frame_len: usize },
    #[error("invalid {frame_len}, length must be in range 1..={}", FRAME_MAX_SIZE)]
    InvalidFrameLen { frame_len: usize },
}

impl FrameErrorKind {
    fn and_consumed(self, consumed: usize) -> FrameProcessorError {
        FrameProcessorError {
            consumed,
            kind: self,
        }
    }
}

impl ConsumedFrameBytes for Result<Frame, FrameProcessorError> {
    fn consumed(&self) -> usize {
        match self {
            Ok(frame) => frame.consumed(),
            Err(e) => e.consumed(),
        }
    }
}

pub struct FrameProcessor {
    state: State,
}

fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
    haystack.iter().position(|b| b == &needle)
}

impl FrameProcessor {
    pub fn new() -> Self {
        Self {
            state: State::Search,
        }
    }
    pub fn process(&mut self, data: &[u8], buf: &mut [u8]) -> Result<Frame, FrameProcessorError> {
        let len = data.len();
        let mut offset = 0;

        while offset < len {
            debug!(
                "Processing frame: state: '{:?}', offset: '{}', data_len: '{}'",
                self.state, offset, len
            );
            match self.state {
                State::Search => {
                    if let Some(pos) = find_byte(FRAME_START1, &data[offset..len]) {
                        offset += pos + 1;
                        self.state = State::FoundS1;
                    } else {
                        // @todo: debug logs are sent between protobuf frames, capture them
                        offset = len;
                    }
                }
                State::FoundS1 => {
                    let b = data[offset];
                    offset += 1;

                    if b == FRAME_START2 {
                        self.state = State::FoundS2;
                    } else if b != FRAME_START1 {
                        self.state = State::Search;
                    };
                }
                State::FoundS2 => {
                    let b = data[offset];
                    offset += 1;
                    self.state = State::FoundMSB(b);
                }
                State::FoundMSB(msb) => {
                    let lsb = data[offset];
                    offset += 1;

                    let frame_len = u16::from_be_bytes([msb, lsb]) as usize;

                    Self::validate_frame_len(frame_len, buf.len()).map_err(|err| {
                        self.state = State::Search;
                        err.and_consumed(offset)
                    })?;

                    self.state = State::Msg {
                        len: frame_len,
                        remaining: frame_len,
                        head: 0,
                    };
                }
                State::Msg {
                    remaining,
                    head,
                    len: msg_len,
                } => {
                    let next_bytes = len - offset;
                    let take = next_bytes.min(remaining);
                    let next_head = head + take;
                    let next_offset = offset + take;

                    buf[head..next_head].copy_from_slice(&data[offset..next_offset]);
                    offset = next_offset;

                    if take == remaining {
                        let frame_len = next_head;
                        self.state = State::Search;
                        return Ok(Frame::Complete {
                            len: frame_len,
                            consumed: offset,
                        });
                    } else {
                        self.state = State::Msg {
                            head: next_head,
                            remaining: remaining - take,
                            len: msg_len,
                        };
                    }
                }
            }
        }
        Ok(Frame::Incomplete { consumed: offset })
    }

    fn validate_frame_len(frame_len: usize, buf_len: usize) -> Result<(), FrameErrorKind> {
        if !(1..=FRAME_MAX_SIZE).contains(&frame_len) {
            Err(FrameErrorKind::InvalidFrameLen { frame_len })
        } else if frame_len > buf_len {
            Err(FrameErrorKind::InsufficientCapacity {
                frame_len,
                capacity: buf_len,
            })
        } else {
            Ok(())
        }
    }
}

impl Default for FrameProcessor {
    fn default() -> Self {
        Self::new()
    }
}
