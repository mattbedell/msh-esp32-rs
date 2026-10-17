use defmt;
use thiserror::Error;

pub const FRAME_START1: u8 = 0x94;
pub const FRAME_START2: u8 = 0xc3;
pub const FRAME_MAX_SIZE: usize = 512; // frame bytes included

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum State {
    Search,
    FoundS1,
    FoundS2,
    FoundMSB(u8),
    Msg { len: usize, remaining: usize, head: usize },
}

pub enum ProcessResult {
    FullFrame(usize),
    Incomplete,
    Err,
}

#[derive(Clone, Copy, Debug, Error)]
#[error("cannot process")]
pub struct ProcessError {
    len: usize,
}

pub struct MeshtasticFrame {
    state: State,
}

pub enum FrameEvent<'a> {
    Proto(&'a [u8]),
    Log(&'a [u8]),
}

fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
    haystack.iter().position(|b| b == &needle)
}

impl MeshtasticFrame {
    pub fn new() -> Self {
        MeshtasticFrame {
            state: State::Search,
        }
    }
    pub fn process(&mut self, data: &[u8], buf: &mut [u8]) -> (usize, ProcessResult) {
        let len = data.len();
        let mut offset = 0;

        while offset < len {
            debug!("Processing frame: state: '{:?}', offset: '{}', data_len: '{}'", self.state, offset, len);
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

                    let proto_len = u16::from_be_bytes([msb, lsb]) as usize;

                    if proto_len == 0 || proto_len > FRAME_MAX_SIZE || proto_len > buf.len() {
                        if proto_len > buf.len() {
                            warn!(
                                "Frame len exceeds buffer: state '{:?}', offset: {}, expected_len: {}, buf_len: {}",
                                self.state,
                                offset,
                                proto_len,
                                buf.len(),
                            );
                        } else {
                            warn!(
                                "Corrupted frame: state '{:?}', offset: {}, expected_len: {}",
                                self.state, offset, proto_len
                            );
                        }
                        self.state = State::Search;
                        return (offset, ProcessResult::Err);
                    } else {
                        self.state = State::Msg {
                            len: proto_len,
                            remaining: proto_len,
                            head: 0,
                        };
                    }
                }
                State::Msg { remaining, head, len: msg_len } => {
                    let next_bytes = len - offset;
                    let take = next_bytes.min(remaining);
                    let next_head = head + take;
                    let next_offset = offset + take;

                    buf[head..next_head].copy_from_slice(&data[offset..next_offset]);
                    offset = next_offset;

                    if take == remaining {
                        let frame_len = next_head;
                        self.state = State::Search;
                        return (offset, ProcessResult::FullFrame(frame_len));
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

        (offset, ProcessResult::Incomplete)
    }
}

impl Default for MeshtasticFrame {
    fn default() -> Self {
        Self::new()
    }
}
