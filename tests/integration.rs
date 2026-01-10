use clean_dns::{
    config::Config, create_plugin_registry, get_entry_plugin, server::Server,
    statistics::Statistics,
};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::net::UdpSocket;

#[tokio::test]
async fn test_dns_server_and_statistics() {
    // 1. Setup Config (Mock or File)
    // We'll create a minimal config programmatically or write a temp file.
    // Since Config::from_file takes a path, temp file is easier.
    use std::io::Write;
    use tempfile::NamedTempFile;

    let mut config_file = NamedTempFile::new().unwrap();
    let config_yaml = r#"
bind: "127.0.0.1:0"
api_port: 0
entry: test_chain
plugins:
  - tag: rejector
    type: reject
    args:
      rcode: 3 # NXDOMAIN

  - tag: test_chain
    type: sequence
    args:
      exec:
        - rejector
"#;
    writeln!(config_file, "{}", config_yaml).unwrap();

    let config_path = config_file.path().to_str().unwrap();
    let mut config = Config::from_file(config_path).expect("Failed to load config");

    // Bind to random port
    config.bind = "127.0.0.1:0".to_string();
    // Wait, Server::new takes SocketAddr.
    // We bind initially to "0" and let OS pick.
    // But Server::new binds.
    // We need to know the port to query it.
    // If Server::new binds, we might need to extract the bound address from it?
    // Server currently returns `Server` struct. If it holds the socket, we might get local_addr?
    // Checking server.rs...
    // Server::new() creates UdpSocket and spawns a loop.
    // It doesn't return the socket or address comfortably unless I modified it.
    // Wait, I refactored server.rs?
    // Let's check server.rs. If I can't get the port, testing is hard.

    // Workaround: Bind a socket first to get a port, then drop it, then start server?
    // Race condition but usually works for local tests.

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let addr = socket.local_addr().unwrap();
    drop(socket);

    let server_addr = addr;

    // Start Server
    let registry = create_plugin_registry(&config).unwrap();
    let entry_plugin = get_entry_plugin(&config, &registry).unwrap();
    let statistics = Arc::new(RwLock::new(Statistics::new()));

    let stats_clone = statistics.clone();

    let server = Server::new(server_addr, entry_plugin, stats_clone);

    // Server::run() is async and infinite loop.
    // We spawn it.
    tokio::spawn(async move {
        server.run().await.unwrap();
    });

    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 2. Query DNS
    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(server_addr).await.unwrap();

    use hickory_proto::op::{Message, MessageType, OpCode, Query};
    use hickory_proto::rr::{Name, RecordType};
    use std::str::FromStr;

    let mut msg = Message::new();
    msg.set_id(1234);
    msg.set_message_type(MessageType::Query);
    msg.set_op_code(OpCode::Query);
    msg.set_recursion_desired(true);
    msg.add_query(Query::query(
        Name::from_str("example.com.").unwrap(),
        RecordType::A,
    ));

    let msg_bytes = msg.to_vec().unwrap();
    client_socket.send(&msg_bytes).await.unwrap();

    let mut buf = [0u8; 512];
    let (len, _) = tokio::time::timeout(Duration::from_secs(1), client_socket.recv_from(&mut buf))
        .await
        .expect("Timeout waiting for response")
        .expect("Recv failed");

    let response = Message::from_vec(&buf[..len]).unwrap();
    assert_eq!(response.id(), 1234);
    // Config used "reject" with rcode 3 (NXDOMAIN)
    assert_eq!(
        response.response_code(),
        hickory_proto::op::ResponseCode::NXDomain
    );

    // 3. Verify Stats
    {
        let s = statistics.read().unwrap();
        // Check "example.com." is recorded
        let stats = s
            .domains
            .get("example.com.")
            .expect("Stats for example.com. not found");
        assert_eq!(stats.count, 1);
        // Resolved IP is empty because we rejected it?
        // Reject plugin doesn't record resolved IP stats manually? No, Server does.
        // Server records resolved IP from RESPONSE.
        // Reject plugin sends response. Does response contain answers? No.
        // So IPs should be empty.
        assert!(stats.ips.is_empty());
    }
}

#[tokio::test]
async fn test_api_stats() {
    use clean_dns::{start_api_server, statistics::Statistics};
    use std::sync::{Arc, RwLock};
    use std::time::Duration;
    use tokio::net::TcpListener;

    let statistics = Arc::new(RwLock::new(Statistics::new()));

    // Find free port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let port = addr.port();
    drop(listener);

    let stats_clone = statistics.clone();
    tokio::spawn(async move {
        // start_api_server binds to 0.0.0.0, so it should catch all interfaces including 127.0.0.1
        start_api_server(stats_clone, port).await.unwrap();
    });

    // Wait for server
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Make request
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/stats", port);

    let resp = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");
    assert!(resp.status().is_success());

    let body = resp.text().await.unwrap();
    // Parse JSON
    let stats_json: serde_json::Value = serde_json::from_str(&body).unwrap();

    // Check structure (domains map is empty)
    assert!(stats_json.get("domains").is_some());
    assert!(stats_json["domains"].as_object().unwrap().is_empty());
}

#[tokio::test]
async fn test_system_resolver_integration() {
    use clean_dns::{
        config::Config, create_plugin_registry, get_entry_plugin, server::Server,
        statistics::Statistics,
    };
    use std::io::Write;
    use std::sync::{Arc, RwLock};
    use std::time::Duration;
    use tempfile::NamedTempFile;
    use tokio::net::UdpSocket;

    let mut config_file = NamedTempFile::new().unwrap();
    let config_yaml = r#"
bind: "127.0.0.1:0"
api_port: 0
entry: sys
plugins:
  - tag: sys
    type: system
"#;
    writeln!(config_file, "{}", config_yaml).unwrap();

    let mut config = Config::from_file(config_file.path().to_str().unwrap()).unwrap();
    config.bind = "127.0.0.1:0".to_string();

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = socket.local_addr().unwrap();
    drop(socket);

    let registry = create_plugin_registry(&config).unwrap();
    let entry_plugin = get_entry_plugin(&config, &registry).unwrap();
    let statistics = Arc::new(RwLock::new(Statistics::new()));

    let server = Server::new(server_addr, entry_plugin, statistics.clone());
    tokio::spawn(async move {
        server.run().await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(server_addr).await.unwrap();

    use hickory_proto::op::{Message, MessageType, OpCode, Query};
    use hickory_proto::rr::{Name, RecordType};
    use std::str::FromStr;

    let mut msg = Message::new();
    msg.set_id(5678);
    msg.set_message_type(MessageType::Query);
    msg.set_op_code(OpCode::Query);
    msg.set_recursion_desired(true);
    msg.add_query(Query::query(
        Name::from_str("google.com.").unwrap(),
        RecordType::A,
    ));

    client_socket.send(&msg.to_vec().unwrap()).await.unwrap();

    let mut buf = [0u8; 512];
    let (len, _) = tokio::time::timeout(Duration::from_secs(2), client_socket.recv_from(&mut buf))
        .await
        .expect("Timeout")
        .expect("Recv failed");

    let response = Message::from_vec(&buf[..len]).unwrap();
    assert_eq!(response.id(), 5678);
    // Success depends on network but it should at least be a valid response
    println!(
        "Integration system resolve rcode: {:?}",
        response.response_code()
    );
}

#[tokio::test]
async fn test_stats_remote_flag() {
    // 1. Setup Config with a mock forwarder (local) and mock proxy forwarder
    // Note: To test "remote", we need `forward` plugin with `socks5` set.
    // But we don't have a real SOCKS5 server easily available in test env without spinning one up.
    // However, the `Forward` plugin sets `is_remote` based on `socks5` config presence,
    // BEFORE the connection is established?
    // Let's check `src/plugins/forward.rs`.
    // It sets `ctx.is_remote = self.socks5.is_some()` ON SUCCESS.
    // If the SOCKS5 connection fails, the request fails.
    // So we need a minimal working SOCKS5 or UDP upstream.
    // Since we can't easily mock SOCKS5 network interaction without external crate or complex setup,
    // we might test the "local" case (is_remote = false) easily.
    // For "remote" testing, it's harder in integration without a proxy.
    // But we can verify default is false.
    // OR, we can verify that `forward` plugin WITHOUT socks5 sets it to false.

    use clean_dns::{
        config::Config, create_plugin_registry, get_entry_plugin, server::Server,
        statistics::Statistics,
    };
    use std::io::Write;
    use std::sync::{Arc, RwLock};
    use std::time::Duration;
    use tempfile::NamedTempFile;
    use tokio::net::UdpSocket;

    let mut config_file = NamedTempFile::new().unwrap();
    // Use `system` plugin which definitely doesn't use socks5 -> is_remote should be false.
    let config_yaml = r#"
bind: "127.0.0.1:0"
api_port: 0
entry: main
plugins:
  - tag: sys
    type: system
    args: {}
  - tag: main
    type: sequence
    args:
      exec: [sys]
"#;
    writeln!(config_file, "{}", config_yaml).unwrap();

    let mut config = Config::from_file(config_file.path().to_str().unwrap()).unwrap();
    config.bind = "127.0.0.1:0".to_string();

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = socket.local_addr().unwrap();
    drop(socket);

    let registry = create_plugin_registry(&config).unwrap();
    let entry_plugin = get_entry_plugin(&config, &registry).unwrap();
    let statistics = Arc::new(RwLock::new(Statistics::new()));

    let server = Server::new(server_addr, entry_plugin, statistics.clone());
    tokio::spawn(async move {
        server.run().await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(server_addr).await.unwrap();

    use hickory_proto::op::{Message, MessageType, OpCode, Query};
    use hickory_proto::rr::{Name, RecordType};
    use std::str::FromStr;

    let mut msg = Message::new();
    msg.set_id(9999);
    msg.set_message_type(MessageType::Query);
    msg.set_op_code(OpCode::Query);
    msg.set_recursion_desired(true);
    msg.add_query(Query::query(
        Name::from_str("example.com.").unwrap(),
        RecordType::A,
    ));

    client_socket.send(&msg.to_vec().unwrap()).await.unwrap();

    let mut buf = [0u8; 512];
    let (len, _) = tokio::time::timeout(Duration::from_secs(2), client_socket.recv_from(&mut buf))
        .await
        .expect("Timeout")
        .expect("Recv failed");

    let response = Message::from_vec(&buf[..len]).unwrap();
    assert_eq!(response.id(), 9999);

    // Verify stats
    {
        let s = statistics.read().unwrap();
        let stats = s.domains.get("example.com.");
        // If system resolver failed (network issue), stats might not have IP.
        // But if it succeeded, it should be remote=false.
        if let Some(entry) = stats {
            // It's possible we didn't get an IP if it failed.
            // But if we did:
            if !entry.ips.is_empty() {
                assert_eq!(entry.last_resolved_remote, false);
            }
        }
    }
}

#[tokio::test]
async fn test_stats_remote_flag_true() {
    use clean_dns::{
        config::Config, create_plugin_registry, get_entry_plugin, server::Server,
        statistics::Statistics,
    };
    use hickory_proto::op::{Message, MessageType, OpCode, Query, ResponseCode};
    use hickory_proto::rr::{Name, RData, Record, RecordType};
    use std::io::Write;
    use std::net::{Ipv4Addr, SocketAddr};
    use std::str::FromStr;
    use std::sync::{Arc, RwLock};
    use std::time::Duration;
    use tempfile::NamedTempFile;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, UdpSocket};

    // 1. Mock SOCKS5 Proxy + Upstream DNS logic
    let proxy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_addr = proxy_listener.local_addr().unwrap();

    tokio::spawn(async move {
        let (mut stream, _) = proxy_listener.accept().await.unwrap();

        // SOCKS5 Handshake
        let mut buf = [0u8; 512];
        let n = stream.read(&mut buf).await.unwrap(); // Expect 05 01 00
        assert!(n >= 3);
        assert_eq!(buf[0], 0x05);

        stream.write_all(&[0x05, 0x00]).await.unwrap();

        let n = stream.read(&mut buf).await.unwrap(); // Expect 05 01 00 01 ...
        assert!(n > 6);
        assert_eq!(buf[1], 0x01); // CMD Connect

        stream
            .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await
            .unwrap();

        // Read DNS Packet Length (2 bytes)
        let mut len_buf = [0u8; 2];
        stream.read_exact(&mut len_buf).await.unwrap();
        let len = u16::from_be_bytes(len_buf) as usize;

        // Read DNS Packet
        let mut dns_buf = vec![0u8; len];
        stream.read_exact(&mut dns_buf).await.unwrap();

        let request = Message::from_vec(&dns_buf).unwrap();
        let query = request.query().unwrap();

        // Construct Response
        let mut response = Message::new();
        response.set_id(request.id());
        response.set_message_type(MessageType::Response);
        response.set_recursion_available(true);
        response.set_response_code(ResponseCode::NoError);
        response.add_query(query.clone());

        // Add A Record answer
        let name = query.name().clone();
        let rdata = RData::A(hickory_proto::rr::rdata::A(Ipv4Addr::new(1, 2, 3, 4)));
        let record = Record::from_rdata(name, 60, rdata);
        response.add_answer(record);

        let response_bytes = response.to_vec().unwrap();
        let response_len = (response_bytes.len() as u16).to_be_bytes();

        // Write Response
        stream.write_all(&response_len).await.unwrap();
        stream.write_all(&response_bytes).await.unwrap();
    });

    // 2. Setup Config
    let mut config_file = NamedTempFile::new().unwrap();
    let config_yaml = format!(
        r#"
bind: "127.0.0.1:0"
api_port: 0
entry: main
plugins:
  - tag: remote_forward
    type: forward
    args:
        upstreams: ["1.1.1.1:53"] 
        socks5: "{}"

  - tag: main
    type: sequence
    args:
      exec: [remote_forward]
"#,
        proxy_addr
    );
    writeln!(config_file, "{}", config_yaml).unwrap();

    let mut config = Config::from_file(config_file.path().to_str().unwrap()).unwrap();
    config.bind = "127.0.0.1:0".to_string();

    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let server_addr = socket.local_addr().unwrap();
    drop(socket);

    let registry = create_plugin_registry(&config).unwrap();
    let entry_plugin = get_entry_plugin(&config, &registry).unwrap();
    let statistics = Arc::new(RwLock::new(Statistics::new()));

    let server = Server::new(server_addr, entry_plugin, statistics.clone());
    tokio::spawn(async move {
        server.run().await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    // 3. Client Query
    let client_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    client_socket.connect(server_addr).await.unwrap();

    let mut msg = Message::new();
    msg.set_id(7777);
    msg.set_message_type(MessageType::Query);
    msg.set_op_code(OpCode::Query);
    msg.set_recursion_desired(true);
    msg.add_query(Query::query(
        Name::from_str("proxied.com.").unwrap(),
        RecordType::A,
    ));

    client_socket.send(&msg.to_vec().unwrap()).await.unwrap();

    let mut buf = [0u8; 512];
    let (len, _) = tokio::time::timeout(Duration::from_secs(2), client_socket.recv_from(&mut buf))
        .await
        .expect("Timeout")
        .expect("Recv failed");

    let response = Message::from_vec(&buf[..len]).unwrap();
    assert_eq!(response.id(), 7777);
    assert_eq!(response.response_code(), ResponseCode::NoError);

    // 4. Verify Stats
    {
        let s = statistics.read().unwrap();
        let stats = s.domains.get("proxied.com.").expect("Stats not found");
        assert!(stats
            .ips
            .contains(&std::net::IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))));
        assert_eq!(stats.last_resolved_remote, true);
    }
}
