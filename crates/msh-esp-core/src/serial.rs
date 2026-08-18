use crate::proto;

const START1: u8 = 0x94;
const START2: u8 = 0xc3;
const MAX_PROTO_FRAME: usize = 512;
const MAX_LOG_FRAME: usize = 256;

enum State {
    Search,
    FoundS1,
    FoundS2,
    FoundMSB,
    FoundLSB,
}

pub struct MeshtasticFrame {
    proto_buf: [u8; MAX_PROTO_FRAME],
    pb_len: usize,
    log_buf: [u8; MAX_LOG_FRAME],
    lb_len: usize,
    state: State
}

impl MeshtasticFrame {
    pub fn process(&mut self, buf: &[u8]) -> usize {
        let len = buf.len();
        let mut offset = 0;
        while offset < len {

            // @todo: placeholder
            offset += 1;
        }


        0
    }
}
