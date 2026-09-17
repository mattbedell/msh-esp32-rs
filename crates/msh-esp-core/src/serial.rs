use core::str::Utf8Error;

use defmt;

pub const FRAME_START1: u8 = 0x94;
pub const FRAME_START2: u8 = 0xc3;
pub const FRAME_MAX_SIZE: usize = 512; // frame bytes included
const MAX_LOG_FRAME: usize = 256;

// struct FrameBuf<const N: usize> {
//     buf: [u8; N],
//     head: usize,
//     tail: usize,
// }

#[derive(Copy, Clone, Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
enum State {
    Search,
    FoundS1,
    FoundS2,
    FoundMSB(u8),
    FullFrame(u16),
    PartialFrame(u16, usize),
}

// pub struct MeshtasticFrame {
//     proto: FrameBuf<FRAME_MAX_SIZE>,
//     log: FrameBuf<MAX_LOG_FRAME>,
// }

pub struct MeshtasticFrame {
    proto_buf: [u8; FRAME_MAX_SIZE],
    log_buf: [u8; MAX_LOG_FRAME],
    lb_len: usize,
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
        info!("frame parser init");
        MeshtasticFrame {
            proto_buf: [0u8; FRAME_MAX_SIZE],
            log_buf: [0u8; MAX_LOG_FRAME],
            lb_len: 0,
            state: State::Search,
        }
    }
    pub fn process(&mut self, data: &[u8], buf: &mut [u8], emit: impl Fn(FrameEvent)) -> usize {
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
                        self.push_log(&data[offset..len], &emit);
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

                    let proto_len = u16::from_be_bytes([msb, lsb]);

                    if proto_len == 0 || proto_len as usize > FRAME_MAX_SIZE {
                        // corrupted msg
                        self.state = State::Search;
                    } else {
                        self.state = State::FullFrame(proto_len);
                    }
                }
                State::FullFrame(remaining) => {
                    info!(
                        "Process frame: state '{:?}', offset: {}",
                        self.state, offset
                    );
                    let next_bytes = len - offset;
                    let rest = remaining as usize;

                    if next_bytes >= rest {
                        info!("EMIT, offset: {}, rest: {}, next_offset: {}, len: {}", offset, rest, offset + rest, len);
                        emit(FrameEvent::Proto(&data[offset..offset + rest]));
                        offset += rest;
                        self.state = State::Search;
                    } else {
                        self.proto_buf[..next_bytes].copy_from_slice(&data[offset..len]);
                        self.state = State::PartialFrame(remaining - next_bytes as u16, next_bytes);
                        offset = len;
                    }
                }
                State::PartialFrame(remaining, partial_len) => {
                    info!(
                        "Process frame: state '{:?}', offset: {}",
                        self.state, offset
                    );
                    let next_bytes = len - offset;
                    let rest = remaining as usize;
                    let take = next_bytes.min(rest);

                    let tail = partial_len + take;

                    if tail > FRAME_MAX_SIZE {
                        warn!(
                            "Partial frame buf exceed max frame size: {}b. Reset.",
                            FRAME_MAX_SIZE
                        );
                        self.state = State::Search;
                        continue;
                    }

                    self.proto_buf[partial_len..tail].copy_from_slice(&data[offset..offset + take]);
                    offset += take;

                    if take == rest {
                        emit(FrameEvent::Proto(&self.proto_buf[..tail]));
                        self.state = State::Search;
                    } else {
                        self.state = State::PartialFrame(remaining - take as u16, tail);
                    }
                }
            }
        }
        offset.min(len)
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

fn try_print_log(buf: &[u8]) -> Result<(), Utf8Error> {
    if let Ok(log_str) = str::from_utf8(buf) {
        info!("{}", log_str);
    } else {
        error!("Cannot convert log buf to utf8");
    }
    Ok(())
}
