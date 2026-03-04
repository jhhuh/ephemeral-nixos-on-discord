/// Live QGA test against a running VM.
/// Run with: cargo test --test qga-live-test -- --ignored --nocapture
use std::path::Path;
use std::time::Duration;

use ephemeral_nixos_bot::qga::QgaClient;

#[tokio::test]
#[ignore] // Only run manually with a VM
async fn test_live_qga() {
    let socket = Path::new("/tmp/microvm/test-vm/qga.sock");
    if !socket.exists() {
        eprintln!("QGA socket not found at {}", socket.display());
        eprintln!("Start a test VM first: see tests/vm-test/");
        return;
    }

    // Retry connection — VM may still be booting (TCG emulation is ~10x slower)
    println!("Connecting to QGA (will retry for up to 5 minutes)...");
    let mut client = None;
    for i in 0..150 {
        match QgaClient::connect(socket).await {
            Ok(c) => {
                println!("Connected after {}s!", i * 2);
                client = Some(c);
                break;
            }
            Err(e) => {
                if i % 10 == 0 {
                    println!("  waiting... ({i}): {e}");
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    let mut client = client.expect("Failed to connect to QGA after 5 minutes");

    // Test exec
    println!("\n--- exec: uname -a ---");
    let output = client.exec("uname -a", Duration::from_secs(30)).await.expect("exec failed");
    println!("stdout: {}", output.stdout);
    println!("exit_code: {}", output.exit_code);

    // Test exec: nixos version
    println!("\n--- exec: nixos-version ---");
    let output = client.exec("nixos-version", Duration::from_secs(30)).await.expect("exec failed");
    println!("stdout: {}", output.stdout);

    // Test exec: list packages
    println!("\n--- exec: which htop ---");
    let output = client.exec("which htop", Duration::from_secs(30)).await.expect("exec failed");
    println!("stdout: {}", output.stdout);

    // Test read_file
    println!("\n--- read_file: /etc/hostname ---");
    let data = client.read_file("/etc/hostname").await.expect("read_file failed");
    println!("content: {}", String::from_utf8_lossy(&data));

    // Test write_file + read_file round-trip
    println!("\n--- write_file + read_file round-trip ---");
    client
        .write_file("/tmp/test-qga.txt", b"hello from QGA test!")
        .await
        .expect("write_file failed");
    let data = client.read_file("/tmp/test-qga.txt").await.expect("read_file failed");
    let content = String::from_utf8_lossy(&data);
    println!("wrote and read back: {}", content);
    assert_eq!(content.trim(), "hello from QGA test!");

    // Test exec: systemctl
    println!("\n--- exec: systemctl status qemu-guest-agent ---");
    let output = client
        .exec("systemctl status qemu-guest-agent", Duration::from_secs(30))
        .await;
    match output {
        Ok(o) => println!("stdout: {}\nexit: {}", o.stdout, o.exit_code),
        Err(e) => println!("(expected) error: {e}"),
    }

    println!("\nAll live QGA tests passed!");
}
