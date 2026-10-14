//! NIIMBOT D110 framing and one-page-per-job print sequence.
//! Protocol reference: iscarelli/niimbot-web-bluetooth (MIT), see third_party/.
//! No platform or UI dependencies; Android can provide another Transport.

use std::time::{Duration, Instant};

pub const D110_MODEL_ID: u16 = 2304;
pub const HEAD_WIDTH: u16 = 96;
pub const DPI: u16 = 203;
pub const MAX_HEIGHT: u16 = 800;
pub const CONNECT_BYTES: &[u8] = &[0x03, 0x55, 0x55, 0xc1, 0x01, 0x01, 0xc1, 0xaa, 0xaa];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub command: u8,
    pub data: Vec<u8>,
}

impl Packet {
    pub fn new(command: u8, data: impl Into<Vec<u8>>) -> Self {
        Self {
            command,
            data: data.into(),
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let len = u8::try_from(self.data.len()).map_err(|_| "Packet exceeds 255 bytes")?;
        let checksum = self
            .data
            .iter()
            .fold(self.command ^ len, |sum, byte| sum ^ byte);
        let mut out = vec![0x55, 0x55, self.command, len];
        out.extend_from_slice(&self.data);
        out.extend_from_slice(&[checksum, 0xaa, 0xaa]);
        Ok(out)
    }
}

/// Notifications can split a frame, contain several frames, or include noise.
#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
    pub rejected_frames: usize,
}

impl Decoder {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Packet> {
        self.buffer.extend_from_slice(bytes);
        let mut packets = Vec::new();
        loop {
            let Some(start) = self.buffer.windows(2).position(|w| w == [0x55, 0x55]) else {
                let keep = self.buffer.last() == Some(&0x55);
                self.buffer.clear();
                if keep {
                    self.buffer.push(0x55);
                }
                break;
            };
            self.buffer.drain(..start);
            if self.buffer.len() < 4 {
                break;
            }
            let len = self.buffer[3] as usize;
            let end = len + 7;
            if self.buffer.len() < end {
                break;
            }
            let checksum = self.buffer[4..4 + len]
                .iter()
                .fold(self.buffer[2] ^ self.buffer[3], |sum, byte| sum ^ byte);
            if self.buffer[end - 2..end] != [0xaa, 0xaa] || self.buffer[4 + len] != checksum {
                self.rejected_frames += 1;
                self.buffer.drain(..1);
                continue;
            }
            packets.push(Packet::new(
                self.buffer[2],
                self.buffer[4..4 + len].to_vec(),
            ));
            self.buffer.drain(..end);
        }
        packets
    }
}

#[derive(Debug)]
pub struct Bitmap {
    pub width: u16,
    pub height: u16,
    bytes: Vec<u8>,
}

impl Bitmap {
    pub fn new(width: u16, height: u16, bytes: Vec<u8>) -> Result<Self, String> {
        if width != HEAD_WIDTH || !(1..=MAX_HEIGHT).contains(&height) {
            return Err(format!(
                "D110 bitmap must be {HEAD_WIDTH} pixels wide and 1–{MAX_HEIGHT} rows high"
            ));
        }
        let expected = usize::from(width / 8) * usize::from(height);
        if bytes.len() != expected {
            return Err(format!(
                "Bitmap has {} bytes, expected {expected}",
                bytes.len()
            ));
        }
        if bytes.iter().all(|b| *b == 0) {
            return Err("Label is empty".into());
        }
        Ok(Self {
            width,
            height,
            bytes,
        })
    }

    pub fn rows(&self) -> Vec<Packet> {
        let stride = usize::from(self.width / 8);
        let mut row = 0usize;
        let mut packets = Vec::new();
        while row < usize::from(self.height) {
            let data = &self.bytes[row * stride..(row + 1) * stride];
            let mut repeat = 1usize;
            while row + repeat < usize::from(self.height)
                && repeat < 200
                && &self.bytes[(row + repeat) * stride..(row + repeat + 1) * stride] == data
            {
                repeat += 1;
            }
            let [hi, lo] = (row as u16).to_be_bytes();
            let black: u16 = data.iter().map(|b| b.count_ones() as u16).sum();
            if black == 0 {
                packets.push(Packet::new(0x84, vec![hi, lo, repeat as u8]));
            } else {
                let [count_lo, count_hi] = black.to_le_bytes();
                let mut payload = vec![hi, lo, 0, count_lo, count_hi, repeat as u8];
                payload.extend_from_slice(data);
                packets.push(Packet::new(0x85, payload));
            }
            row += repeat;
        }
        packets
    }
}

pub fn model_id(data: &[u8]) -> Option<u16> {
    match data {
        [hi, lo, ..] => Some(u16::from_be_bytes([*hi, *lo])),
        [hi] => Some(u16::from(*hi) << 8),
        [] => None,
    }
}

pub fn protocol_version(data: &[u8]) -> Option<u8> {
    if data.len() < 13 {
        return None;
    }
    match u16::from(data[11]) * 100 + u16::from(data[12]) {
        204..=299 => Some(3),
        300..=301 => Some(4),
        302.. => Some(5),
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PrintStatus {
    pub page: u16,
    pub print_percent: u8,
    pub feed_percent: u8,
}

impl PrintStatus {
    pub fn decode(data: &[u8]) -> Result<Self, String> {
        if data.len() < 4 {
            return Err("Truncated print status".into());
        }
        Ok(Self {
            page: u16::from_be_bytes([data[0], data[1]]),
            print_percent: data[2].min(100),
            feed_percent: data[3].min(100),
        })
    }
}

pub trait Transport {
    fn send(&mut self, packet: &Packet) -> Result<(), String>;
    fn request(
        &mut self,
        packet: &Packet,
        expected: u8,
        timeout: Duration,
    ) -> Result<Packet, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Configuring,
    Uploading,
    Printing,
    Complete,
}

#[derive(Clone, Copy, Debug)]
pub struct Progress {
    pub copy: u8,
    pub copies: u8,
    pub stage: Stage,
    pub percent: u8,
}

pub struct PrintOptions {
    pub density: u8,
    pub copies: u8,
    pub completion_timeout: Duration,
    pub poll_interval: Duration,
}

impl Default for PrintOptions {
    fn default() -> Self {
        Self {
            density: 3,
            copies: 1,
            completion_timeout: Duration::from_secs(25),
            poll_interval: Duration::from_millis(150),
        }
    }
}

#[derive(Debug)]
pub struct PrintFailure {
    pub message: String,
    pub completed_copies: u8,
}

fn ack<T: Transport>(
    transport: &mut T,
    command: u8,
    data: Vec<u8>,
    response: u8,
    seconds: u64,
) -> Result<(), String> {
    let packet = transport.request(
        &Packet::new(command, data),
        response,
        Duration::from_secs(seconds),
    )?;
    if packet.command != response || packet.data.first() != Some(&1) {
        return Err(format!(
            "Printer refused command 0x{command:02X}: {:02X?}",
            packet.data
        ));
    }
    Ok(())
}

/// D110 accepts multi-page settings but prints only the first page. Each copy
/// therefore gets a complete, separate job. Never retry an uncertain print.
pub fn print<T: Transport>(
    transport: &mut T,
    bitmap: &Bitmap,
    options: &PrintOptions,
    mut progress: impl FnMut(Progress),
    cancelled: impl Fn() -> bool,
) -> Result<u8, PrintFailure> {
    let failure = |message: String, completed_copies| PrintFailure {
        message,
        completed_copies,
    };
    if !(1..=5).contains(&options.density) || !(1..=20).contains(&options.copies) {
        return Err(failure(
            "Density must be 1–5; copies must be 1–20".into(),
            0,
        ));
    }
    let rows = bitmap.rows();
    for copy in 1..=options.copies {
        if cancelled() {
            return Err(failure("Printing cancelled".into(), copy - 1));
        }
        progress(Progress {
            copy,
            copies: options.copies,
            stage: Stage::Configuring,
            percent: 0,
        });
        let setup = ack(transport, 0x21, vec![options.density], 0x31, 2)
            .and_then(|_| ack(transport, 0x23, vec![1], 0x33, 2));
        if let Err(message) = setup {
            return Err(failure(message, copy - 1));
        }
        let page_result = (|| {
            ack(transport, 0x01, vec![0, 1, 0, 0, 0, 0, 0], 0x02, 3)?;
            ack(transport, 0x03, vec![1], 0x04, 2)?;
            let [h_hi, h_lo] = bitmap.height.to_be_bytes();
            let [w_hi, w_lo] = bitmap.width.to_be_bytes();
            ack(transport, 0x13, vec![h_hi, h_lo, w_hi, w_lo, 0, 1], 0x14, 3)?;
            for packet in &rows {
                if cancelled() {
                    return Err("Printing cancelled; current label may be incomplete".into());
                }
                transport.send(packet)?;
                let row = usize::from(u16::from_be_bytes([packet.data[0], packet.data[1]]));
                let repeat = usize::from(packet.data[if packet.command == 0x84 { 2 } else { 5 }]);
                progress(Progress {
                    copy,
                    copies: options.copies,
                    stage: Stage::Uploading,
                    percent: ((row + repeat) * 100 / usize::from(bitmap.height)) as u8,
                });
            }
            ack(transport, 0xe3, vec![1], 0xe4, 10)?;
            let deadline = Instant::now() + options.completion_timeout;
            let mut last_page = None;
            loop {
                if cancelled() {
                    return Err("Printing cancelled; current label may already have printed".into());
                }
                if Instant::now() >= deadline {
                    return Err(format!("Print completion not confirmed (last page: {last_page:?}). Check the physical label before printing again"));
                }
                let response = transport.request(
                    &Packet::new(0xa3, vec![1]),
                    0xb3,
                    Duration::from_secs(2).min(deadline.saturating_duration_since(Instant::now())),
                )?;
                let status = PrintStatus::decode(&response.data)?;
                last_page = Some(status.page);
                progress(Progress {
                    copy,
                    copies: options.copies,
                    stage: Stage::Printing,
                    percent: status.print_percent,
                });
                if status.page >= 1 {
                    return Ok(());
                }
                std::thread::sleep(options.poll_interval);
            }
        })();
        // Feed/retract even after a timeout or cancellation, without reporting success.
        let close_result = ack(transport, 0xf3, vec![1], 0xf4, 3);
        if let Err(message) = page_result {
            let message = match close_result {
                Ok(()) => message,
                Err(close) => format!("{message}. Job cleanup failed: {close}"),
            };
            return Err(failure(message, copy - 1));
        }
        if let Err(message) = close_result {
            return Err(failure(message, copy - 1));
        }
        progress(Progress {
            copy,
            copies: options.copies,
            stage: Stage::Complete,
            percent: 100,
        });
    }
    Ok(options.copies)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn bitmap(height: u16) -> Bitmap {
        Bitmap::new(96, height, vec![0x80; 12 * usize::from(height)]).unwrap()
    }

    #[test]
    fn connection_frame_matches_wire_capture() {
        assert_eq!(
            &Packet::new(0xc1, vec![1]).encode().unwrap(),
            &CONNECT_BYTES[1..]
        );
    }

    #[test]
    fn rejects_oversized_packet() {
        assert!(Packet::new(1, vec![0; 256]).encode().is_err());
    }

    #[test]
    fn decodes_every_possible_split() {
        let packet = Packet::new(0x48, vec![9, 0]);
        let wire = packet.encode().unwrap();
        for split in 1..wire.len() {
            let mut decoder = Decoder::default();
            assert!(decoder.feed(&wire[..split]).is_empty());
            assert_eq!(decoder.feed(&wire[split..]), vec![packet.clone()]);
        }
    }

    #[test]
    fn resynchronizes_after_corrupt_crc_and_handles_coalesced_frames() {
        let packet = Packet::new(0xb3, vec![0, 1, 100, 100]);
        let good = packet.encode().unwrap();
        let mut bad = good.clone();
        bad[8] ^= 1;
        let mut decoder = Decoder::default();
        let bytes = [vec![0, 3, 55], bad, good.clone(), good].concat();
        assert_eq!(decoder.feed(&bytes), vec![packet.clone(), packet]);
        assert_eq!(decoder.rejected_frames, 1);
    }

    #[test]
    fn header_split_after_noise_is_retained() {
        let packet = Packet::new(0x48, vec![9, 0]);
        let mut decoder = Decoder::default();
        assert!(decoder.feed(&[9, 8, 0x55]).is_empty());
        assert_eq!(decoder.feed(&packet.encode().unwrap()[1..]), vec![packet]);
    }

    #[test]
    fn rows_use_little_endian_black_count_and_runs() {
        let packets = bitmap(3).rows();
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].command, 0x85);
        assert_eq!(&packets[0].data[..6], &[0, 0, 0, 12, 0, 3]);
        assert_eq!(&packets[0].data[6..], &[0x80; 12]);
    }

    #[test]
    fn runs_stop_at_200_and_preserve_big_endian_row_numbers() {
        let packets = bitmap(401).rows();
        assert_eq!(
            packets.iter().map(|p| p.data[5]).collect::<Vec<_>>(),
            vec![200, 200, 1]
        );
        assert_eq!(&packets[2].data[..2], &[1, 144]);
    }

    #[test]
    fn empty_rows_are_encoded_separately() {
        let mut bytes = vec![0; 36];
        bytes[12] = 0x80;
        let packets = Bitmap::new(96, 3, bytes).unwrap().rows();
        assert_eq!(
            packets.iter().map(|p| p.command).collect::<Vec<_>>(),
            vec![0x84, 0x85, 0x84]
        );
        assert_eq!(packets[0].data, vec![0, 0, 1]);
    }

    #[test]
    fn malformed_and_empty_images_are_rejected() {
        assert!(Bitmap::new(120, 1, vec![255; 15]).is_err());
        assert!(Bitmap::new(96, 0, vec![]).is_err());
        assert!(Bitmap::new(96, 801, vec![255; 9612]).is_err());
        assert!(Bitmap::new(96, 1, vec![255; 11]).is_err());
        assert!(Bitmap::new(96, 1, vec![0; 12]).is_err());
    }

    #[test]
    fn handles_short_identity_and_protocol_responses() {
        assert_eq!(model_id(&[9]), Some(2304));
        assert_eq!(model_id(&[9, 0]), Some(2304));
        assert_eq!(model_id(&[]), None);
        assert_eq!(protocol_version(&[0; 2]), None);
        let mut data = [0; 13];
        data[11] = 3;
        data[12] = 1;
        assert_eq!(protocol_version(&data), Some(4));
    }

    #[derive(Default)]
    struct Mock {
        sent: Vec<Packet>,
        statuses: VecDeque<Vec<u8>>,
        fail_on: Option<u8>,
        deny_on: Option<u8>,
    }
    impl Transport for Mock {
        fn send(&mut self, packet: &Packet) -> Result<(), String> {
            self.sent.push(packet.clone());
            if self.fail_on == Some(packet.command) {
                return Err("Simulated transport failure".into());
            }
            Ok(())
        }
        fn request(
            &mut self,
            packet: &Packet,
            expected: u8,
            _: Duration,
        ) -> Result<Packet, String> {
            self.send(packet)?;
            let data = if packet.command == 0xa3 {
                self.statuses.pop_front().unwrap_or(vec![0, 1, 100, 100])
            } else {
                vec![if self.deny_on == Some(packet.command) {
                    0
                } else {
                    1
                }]
            };
            Ok(Packet::new(expected, data))
        }
    }

    #[test]
    fn prints_each_copy_in_a_separate_job() {
        let mut mock = Mock::default();
        let options = PrintOptions {
            copies: 3,
            ..PrintOptions::default()
        };
        assert_eq!(
            print(&mut mock, &bitmap(2), &options, |_| {}, || false).unwrap(),
            3
        );
        assert_eq!(mock.sent.iter().filter(|p| p.command == 0xf3).count(), 3);
        for packet in &mock.sent {
            if packet.command == 0x01 {
                assert_eq!(packet.data, vec![0, 1, 0, 0, 0, 0, 0]);
            }
            if packet.command == 0x13 {
                assert_eq!(packet.data, vec![0, 2, 0, 96, 0, 1]);
            }
        }
    }

    #[test]
    fn waits_for_page_counter_before_ending_job() {
        let mut mock = Mock {
            statuses: VecDeque::from([vec![0, 0, 100, 0], vec![0, 1, 100, 100]]),
            ..Mock::default()
        };
        let options = PrintOptions {
            poll_interval: Duration::ZERO,
            ..PrintOptions::default()
        };
        print(&mut mock, &bitmap(1), &options, |_| {}, || false).unwrap();
        assert_eq!(
            &mock.sent.iter().map(|p| p.command).collect::<Vec<_>>()[7..],
            &[0xa3, 0xa3, 0xf3]
        );
    }

    #[test]
    fn failure_cleans_up_and_does_not_send_more_copies() {
        let mut mock = Mock {
            fail_on: Some(0xe3),
            ..Mock::default()
        };
        let options = PrintOptions {
            copies: 3,
            ..PrintOptions::default()
        };
        let err = print(&mut mock, &bitmap(1), &options, |_| {}, || false).unwrap_err();
        assert_eq!(err.completed_copies, 0);
        assert_eq!(mock.sent.last().unwrap().command, 0xf3);
        assert_eq!(mock.sent.iter().filter(|p| p.command == 0x01).count(), 1);
    }

    #[test]
    fn completion_timeout_is_not_success_and_still_closes_job() {
        let mut mock = Mock::default();
        let options = PrintOptions {
            completion_timeout: Duration::ZERO,
            ..PrintOptions::default()
        };
        let err = print(&mut mock, &bitmap(1), &options, |_| {}, || false).unwrap_err();
        assert!(err.message.contains("not confirmed"));
        assert_eq!(mock.sent.last().unwrap().command, 0xf3);
    }

    #[test]
    fn negative_ack_stops_before_starting_print() {
        let mut mock = Mock {
            deny_on: Some(0x21),
            ..Mock::default()
        };
        assert!(print(
            &mut mock,
            &bitmap(1),
            &PrintOptions::default(),
            |_| {},
            || false
        )
        .is_err());
        assert_eq!(mock.sent.len(), 1);
    }

    #[test]
    fn cancellation_before_start_does_not_touch_transport() {
        let mut mock = Mock::default();
        assert!(print(
            &mut mock,
            &bitmap(1),
            &PrintOptions::default(),
            |_| {},
            || true
        )
        .is_err());
        assert!(mock.sent.is_empty());
    }

    #[test]
    fn invalid_options_do_not_touch_transport() {
        let mut mock = Mock::default();
        let options = PrintOptions {
            density: 6,
            ..PrintOptions::default()
        };
        assert!(print(&mut mock, &bitmap(1), &options, |_| {}, || false).is_err());
        assert!(mock.sent.is_empty());
    }

    #[test]
    fn cancellation_during_upload_closes_job_and_stops_remaining_rows() {
        use std::cell::Cell;
        let cancelled = Cell::new(false);
        let mut mock = Mock::default();
        let err = print(
            &mut mock,
            &bitmap(401),
            &PrintOptions::default(),
            |p| {
                if p.stage == Stage::Uploading {
                    cancelled.set(true);
                }
            },
            || cancelled.get(),
        )
        .unwrap_err();
        assert_eq!(err.completed_copies, 0);
        assert_eq!(mock.sent.iter().filter(|p| p.command == 0x85).count(), 1);
        assert_eq!(mock.sent.last().unwrap().command, 0xf3);
    }

    #[test]
    fn cancellation_between_copies_reports_already_completed_copy() {
        use std::cell::Cell;
        let cancelled = Cell::new(false);
        let mut mock = Mock::default();
        let options = PrintOptions {
            copies: 3,
            ..PrintOptions::default()
        };
        let err = print(
            &mut mock,
            &bitmap(1),
            &options,
            |p| {
                if p.stage == Stage::Complete {
                    cancelled.set(true);
                }
            },
            || cancelled.get(),
        )
        .unwrap_err();
        assert_eq!(err.completed_copies, 1);
        assert_eq!(mock.sent.iter().filter(|p| p.command == 0x01).count(), 1);
    }

    #[test]
    fn malformed_status_and_missing_final_ack_are_never_success() {
        let mut mock = Mock {
            statuses: VecDeque::from([vec![0, 1]]),
            ..Mock::default()
        };
        assert!(print(
            &mut mock,
            &bitmap(1),
            &PrintOptions::default(),
            |_| {},
            || false
        )
        .is_err());
        assert_eq!(mock.sent.last().unwrap().command, 0xf3);
        let mut mock = Mock {
            fail_on: Some(0xf3),
            ..Mock::default()
        };
        assert!(print(
            &mut mock,
            &bitmap(1),
            &PrintOptions::default(),
            |_| {},
            || false
        )
        .is_err());
    }

    #[test]
    fn uncertain_print_start_is_closed_without_retransmission() {
        let mut mock = Mock {
            fail_on: Some(0x01),
            ..Mock::default()
        };
        assert!(print(
            &mut mock,
            &bitmap(1),
            &PrintOptions::default(),
            |_| {},
            || false
        )
        .is_err());
        assert_eq!(
            mock.sent.iter().map(|p| p.command).collect::<Vec<_>>(),
            vec![0x21, 0x23, 0x01, 0xf3]
        );
    }
}
