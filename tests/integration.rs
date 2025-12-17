use clean_dns::{
    config::Config, create_plugin_registry, get_entry_plugin, server::Server,
    statistics::Statistics,
};
use std::net::SocketAddr;
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
