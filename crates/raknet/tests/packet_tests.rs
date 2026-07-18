use rustmine_raknet::*;
use std::net::SocketAddr;

// ── offline packet round-trips ─────────────────────────────────────

#[test]
fn unconnected_ping_decode() {
    let mut buf = vec![0x01]; // ID_UNCONNECTED_PING
    buf.extend_from_slice(&0i64.to_be_bytes()); // time
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&123456789i64.to_be_bytes()); // client guid

    let packet = decode_offline(&buf).unwrap();
    match packet {
        OfflinePacket::UnconnectedPing {
            time: _,
            client_guid,
        } => assert_eq!(client_guid, 123456789),
        _ => panic!("expected UnconnectedPing"),
    }
}

#[test]
fn unconnected_pong_encode_roundtrip() {
    let motd = "MCPE;Test;1001;1.26.30;0;20;12345;World;Survival;1;";
    let pong = OfflinePacket::UnconnectedPong {
        time: 0,
        server_guid: 987654321,
        motd: motd.to_string(),
    };
    let raw = encode_offline(&pong);
    assert_eq!(raw[0], id::UNCONNECTED_PONG);
}

#[test]
fn open_conn_request_1_decode() {
    let mut buf = vec![0x05]; // ID_OPEN_CONNECTION_REQUEST_1
    buf.extend_from_slice(&MAGIC);
    buf.push(11); // protocol version
    buf.extend_from_slice(&[0u8; 1460]); // padding for MTU

    let packet = decode_offline(&buf).unwrap();
    match packet {
        OfflinePacket::OpenConnectionRequest1 {
            protocol_version,
            mtu: _,
        } => assert_eq!(protocol_version, 11),
        _ => panic!("expected OpenConnectionRequest1"),
    }
}

#[test]
fn open_conn_reply_1_encode_roundtrip() {
    let reply = OfflinePacket::OpenConnectionReply1 {
        server_guid: 555,
        use_encryption: false,
        mtu: 1492,
    };
    let raw = encode_offline(&reply);
    assert_eq!(raw[0], id::OPEN_CONNECTION_REPLY_1);
    assert_eq!(raw.len(), 28); // 1 id + 16 magic + 8 guid + 1 enc + 2 mtu
}

#[test]
fn open_conn_reply_2_encode_roundtrip() {
    let addr: SocketAddr = "127.0.0.1:19132".parse().unwrap();
    let reply = OfflinePacket::OpenConnectionReply2 {
        server_guid: 42,
        client_address: addr,
        mtu: 1492,
        encryption_enabled: false,
    };
    let raw = encode_offline(&reply);
    assert_eq!(raw[0], id::OPEN_CONNECTION_REPLY_2);
}

// ── frame round-trips ──────────────────────────────────────────────

#[test]
fn frame_unreliable_roundtrip() {
    let frame = Frame {
        reliability: Reliability::Unreliable,
        is_split: false,
        reliable_index: None,
        sequence_index: None,
        order_index: None,
        order_channel: 0,
        split_count: None,
        split_id: None,
        split_index: None,
        body: b"hello world".to_vec(),
    };
    let raw = encode_frame(&frame);
    let decoded = decode_frame(&raw).unwrap().0;
    assert_eq!(decoded.reliability, Reliability::Unreliable);
    assert_eq!(decoded.body, b"hello world");
}

#[test]
fn frame_reliable_roundtrip() {
    let frame = Frame {
        reliability: Reliability::Reliable,
        is_split: false,
        reliable_index: Some(42),
        sequence_index: None,
        order_index: None,
        order_channel: 0,
        split_count: None,
        split_id: None,
        split_index: None,
        body: b"reliable payload".to_vec(),
    };
    let raw = encode_frame(&frame);
    let decoded = decode_frame(&raw).unwrap().0;
    assert_eq!(decoded.reliability, Reliability::Reliable);
    assert_eq!(decoded.reliable_index, Some(42));
    assert_eq!(decoded.body, b"reliable payload");
}

#[test]
fn frame_reliable_ordered_roundtrip() {
    let frame = Frame {
        reliability: Reliability::ReliableOrdered,
        is_split: false,
        reliable_index: Some(10),
        sequence_index: None,
        order_index: Some(5),
        order_channel: 2,
        split_count: None,
        split_id: None,
        split_index: None,
        body: vec![1, 2, 3, 4],
    };
    let raw = encode_frame(&frame);
    let decoded = decode_frame(&raw).unwrap().0;
    assert_eq!(decoded.reliability, Reliability::ReliableOrdered);
    assert_eq!(decoded.reliable_index, Some(10));
    assert_eq!(decoded.order_index, Some(5));
    assert_eq!(decoded.order_channel, 2);
    assert_eq!(decoded.body, vec![1, 2, 3, 4]);
}

// ── datagram round-trips ───────────────────────────────────────────

#[test]
fn datagram_single_frame_roundtrip() {
    let frame = Frame {
        reliability: Reliability::Unreliable,
        is_split: false,
        reliable_index: None,
        sequence_index: None,
        order_index: None,
        order_channel: 0,
        split_count: None,
        split_id: None,
        split_index: None,
        body: b"game packet".to_vec(),
    };
    let entry = FrameSetEntry {
        sequence_number: 7,
        frames: vec![frame],
    };
    let raw = encode_datagram(&entry);
    let decoded = decode_datagram(&raw).unwrap();
    assert_eq!(decoded.sequence_number, 7);
    assert_eq!(decoded.frames.len(), 1);
    assert_eq!(decoded.frames[0].body, b"game packet");
}

// ── ACK round-trips ────────────────────────────────────────────────

#[test]
fn ack_encode_decode() {
    let ack = Ack {
        sequences: vec![SequenceRange { start: 1, end: 3 }],
    };
    let raw = encode_ack(&ack, false);
    let decoded = decode_ack(&raw).unwrap();
    assert_eq!(decoded.sequences.len(), 1);
    assert_eq!(decoded.sequences[0].start, 1);
    assert_eq!(decoded.sequences[0].end, 3);
}

#[test]
fn nack_encode_decode() {
    let nack = Ack {
        sequences: vec![
            SequenceRange { start: 0, end: 0 },
            SequenceRange { start: 5, end: 7 },
        ],
    };
    let raw = encode_ack(&nack, true);
    let decoded = decode_nack(&raw).unwrap();
    assert_eq!(decoded.sequences.len(), 2);
}

// ── reliability helpers ────────────────────────────────────────────

#[test]
fn reliability_from_raw() {
    assert_eq!(Reliability::from_raw(0), Some(Reliability::Unreliable));
    assert_eq!(Reliability::from_raw(2), Some(Reliability::Reliable));
    assert_eq!(Reliability::from_raw(3), Some(Reliability::ReliableOrdered));
    assert_eq!(Reliability::from_raw(7), None);
}

#[test]
fn frame_type_from_byte() {
    assert_eq!(FrameType::from_byte(0xc0), Some(FrameType::Ack));
    assert_eq!(FrameType::from_byte(0xa0), Some(FrameType::Nack));
    assert_eq!(FrameType::from_byte(0x80), Some(FrameType::FrameSet));
    assert_eq!(FrameType::from_byte(0x8f), Some(FrameType::FrameSet));
    assert_eq!(FrameType::from_byte(0x00), None);
}

// ── rejection of bad input ─────────────────────────────────────────

#[test]
fn reject_empty_buffer() {
    assert!(parse_packet(&[]).is_err());
}

#[test]
fn reject_unknown_packet_id() {
    assert!(parse_packet(&[0xff]).is_err());
}

#[test]
fn reject_truncated_offline() {
    assert!(decode_offline(&[0x01, 0x00]).is_err());
}

#[test]
fn reject_bad_magic() {
    let mut buf = vec![0x01];
    buf.extend_from_slice(&0i64.to_be_bytes());
    buf.extend_from_slice(&[0u8; 16]); // wrong magic
    buf.extend_from_slice(&0i64.to_be_bytes());
    assert!(decode_offline(&buf).is_err());
}

// ── split frame header roundtrip ───────────────────────────────────

#[test]
fn frame_with_split_flag_roundtrips() {
    let frame = Frame {
        reliability: Reliability::ReliableOrdered,
        is_split: true,
        reliable_index: Some(1),
        sequence_index: None,
        order_index: Some(2),
        order_channel: 0,
        split_count: Some(4),
        split_id: Some(99),
        split_index: Some(3),
        body: b"chunk3".to_vec(),
    };
    let raw = encode_frame(&frame);
    let decoded = decode_frame(&raw).unwrap().0;
    assert!(decoded.is_split);
    assert_eq!(decoded.split_count, Some(4));
    assert_eq!(decoded.split_id, Some(99));
    assert_eq!(decoded.split_index, Some(3));
    assert_eq!(decoded.body, b"chunk3");
}

#[test]
fn split_into_frames_produces_multiple_pieces() {
    let payload: Vec<u8> = (0..4096u32).map(|i| (i & 0xff) as u8).collect();
    let pieces = split_into_frames(&payload, 576, 12, Reliability::ReliableOrdered, 0);
    assert!(pieces.len() > 1);
    let total: usize = pieces.iter().map(|p| p.body.len()).sum();
    assert_eq!(total, payload.len());
    assert!(pieces.iter().all(|p| p.is_split && p.split_id == Some(12)));
    // Indices must be 0..n unique.
    let mut indices: Vec<u32> = pieces.iter().map(|p| p.split_index.unwrap()).collect();
    indices.sort();
    for (i, idx) in indices.iter().enumerate() {
        assert_eq!(*idx, i as u32);
    }
}
