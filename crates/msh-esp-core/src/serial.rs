use defmt;
use thiserror::Error;

pub const FRAME_START1: u8 = 0x94;
pub const FRAME_START2: u8 = 0xc3;
pub const FRAME_MAX_SIZE: usize = 512; // frame bytes included
const MAX_LOG_FRAME: usize = 256;

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum State {
    Search,
    FoundS1,
    FoundS2,
    FoundMSB(u8),
    Msg { remaining: usize },
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
    log_buf: [u8; MAX_LOG_FRAME],
    lb_len: usize,
    state: State,
    head: usize,
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
        info!("frame parser init");
        MeshtasticFrame {
            log_buf: [0u8; MAX_LOG_FRAME],
            lb_len: 0,
            state: State::Search,
            head: 0,
        }
    }
    pub fn process(
        &mut self,
        data: &[u8],
        buf: &mut [u8],
    ) -> (usize, ProcessResult) {
        let len = data.len();
        let mut offset = 0;
        info!("begin frame process {} {}", len, offset);

        while offset < len {
            info!("offset: {}, state: {}", offset, self.state);
            match self.state {
                State::Search => {
                    if let Some(pos) = find_byte(FRAME_START1, &data[offset..len]) {
                        offset += pos + 1;
                        self.state = State::FoundS1;
                    } else {
                        info!("PUSH LOG");
                        // push to log
                        // self.push_log(&data[offset..len], &emit);
                        offset = len;
                    }
                }
                State::FoundS1 => {
                    info!(
                        "Process frame: state '{:?}', offset: {}",
                        self.state, offset
                    );
                    let b = data[offset];
                    offset += 1;

                    if b == FRAME_START2 {
                        self.state = State::FoundS2;
                    } else if b != FRAME_START1 {
                        self.state = State::Search;
                    };
                }
                State::FoundS2 => {
                    info!(
                        "Process frame: state '{:?}', offset: {}",
                        self.state, offset
                    );
                    let b = data[offset];
                    offset += 1;
                    self.state = State::FoundMSB(b);
                }
                State::FoundMSB(msb) => {
                    info!(
                        "Process frame: state '{:?}', offset: {}",
                        self.state, offset
                    );
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
                        self.head = 0;
                        self.state = State::Msg {
                            remaining: proto_len,
                        };
                    }
                }
                State::Msg { remaining } => {
                    info!(
                        "Process frame: state '{:?}', offset: {}",
                        self.state, offset
                    );

                    let next_bytes = len - offset;
                    let take = next_bytes.min(remaining);

                    buf[self.head..self.head + take].copy_from_slice(&data[offset..offset + take]);
                    self.head += take;
                    offset += take;

                    if take == remaining {
                        let frame_len = self.head;
                        self.head = 0;
                        self.state = State::Search;
                        return (offset, ProcessResult::FullFrame(frame_len));
                    } else {
                        self.state = State::Msg {
                            remaining: remaining - take,
                        };
                    }
                }
            }
        }

        (offset, ProcessResult::Incomplete)
    }

    pub fn push_log(&mut self, buf: &[u8], emit: impl Fn(FrameEvent)) {
        let mut offset = 0;
        while offset < buf.len() {
            if let Some(pos) = find_byte(b'\n', &buf[offset..]) {
                info!(
                    "LOG found: offset: {}, pos: {}, len: {}",
                    offset,
                    pos,
                    buf.len()
                );
                let end = offset + pos + 1;
                let log = &buf[offset..end];

                if self.lb_len == 0 {
                    emit(FrameEvent::Log(log));
                    offset = end;
                } else {
                    let cap = MAX_LOG_FRAME.saturating_sub(self.lb_len);
                    let take = log.len();

                    if take > cap {
                        warn!("Log buffer full, no newline found, flushing...");
                        self.lb_len = 0;
                        emit(FrameEvent::Log(&self.log_buf));
                        emit(FrameEvent::Log(log));
                        offset += take;
                    } else {
                        self.log_buf[self.lb_len..self.lb_len + take].copy_from_slice(log);
                        emit(FrameEvent::Log(&self.log_buf[..self.lb_len + take]));
                        self.lb_len = 0;
                        offset += take;
                    }
                }
            // @todo: dry this up
            } else {
                let log = &buf[offset..];
                let cap = MAX_LOG_FRAME.saturating_sub(self.lb_len);
                let take = log.len();
                if take > cap {
                    warn!("Log buffer full, no newline found, flushing...");
                    self.lb_len = 0;
                    emit(FrameEvent::Log(&self.log_buf));
                    // @todo: cap increased after flush, dont need to log this necessarily yet
                    emit(FrameEvent::Log(log));
                    offset += take;
                } else {
                    self.log_buf[self.lb_len..self.lb_len + take].copy_from_slice(log);
                    self.lb_len += take;
                    offset += take;
                }
            }
        }
    }
}

impl Default for MeshtasticFrame {
    fn default() -> Self {
        Self::new()
    }
}

// fn try_print_log(buf: &[u8]) -> Result<(), Utf8Error> {
//     if let Ok(log_str) = str::from_utf8(buf) {
//         info!("{}", log_str);
//     } else {
//         error!("Cannot convert log buf to utf8");
//     }
//     Ok(())
// }
