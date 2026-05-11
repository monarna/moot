use std::path::PathBuf;
use tokio::process::{Command, Child};
use tokio::io::{AsyncBufReadExt, BufReader};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::fs;

pub struct TorManager {
    #[allow(dead_code)]
    data_dir: PathBuf,
    process: Arc<Mutex<Option<Child>>>,
    onion_address: Arc<Mutex<Option<String>>>,
    socks_port: Arc<Mutex<u16>>,
}

impl TorManager {
    pub async fn start(data_dir: &str, extra_config: Option<&str>, tor_binary: &str, http_port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = PathBuf::from(data_dir);
        fs::create_dir_all(&data_dir)?;

        let hs_dir = data_dir.join("hidden_service");
        fs::create_dir_all(&hs_dir)?;

        let tor_data_dir = data_dir.join("tor_data");
        fs::create_dir_all(&tor_data_dir)?;

        let socks_port = 19050;
        let torrc_path = data_dir.join("torrc");
        let mut torrc_content = format!(
            "SOCKSPort {}\n\
             DataDirectory {}\n\
             HiddenServiceDir {}\n\
             HiddenServicePort 80 127.0.0.1:{}\n\
             Log notice stdout\n\
             CircuitBuildTimeout 30\n\
             LearnCircuitBuildTimeout 0\n\
             SafeLogging 0\n",
            socks_port,
            tor_data_dir.display(),
            hs_dir.display(),
            http_port,
        );
        if let Some(extra) = extra_config {
            torrc_content.push_str(extra);
            torrc_content.push('\n');
        }
        fs::write(&torrc_path, torrc_content)?;

        let mut child = Command::new(tor_binary)
            .arg("-f")
            .arg(&torrc_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn tor: {}. Is tor installed?", e))?;

        let hostname_file = hs_dir.join("hostname");
        let onion_address = Arc::new(Mutex::new(None));
        let onion_address_clone = onion_address.clone();

        let reader = BufReader::new(child.stdout.take().unwrap());
        let mut lines = reader.lines();
        let mut bootstrapped = false;

        tokio::time::timeout(std::time::Duration::from_secs(120), async {
            while let Some(line) = lines.next_line().await.transpose() {
                let line = line.unwrap_or_default();
                println!("[tor] {}", line);

                if line.contains("Opened HiddenService") || line.contains("Tor has successfully opened a circuit") {
                    bootstrapped = true;
                }

                if line.contains("HiddenServiceDir") && line.contains("hostname") && hs_dir.join("hostname").exists() {
                    loop {
                        if let Ok(addr) = fs::read_to_string(&hostname_file) {
                            let addr = addr.trim().to_string();
                            if !addr.is_empty() {
                                *onion_address_clone.lock().await = Some(addr);
                                break;
                            }
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }

                if bootstrapped && onion_address_clone.lock().await.is_some() {
                    break;
                }
            }
        }).await.map_err(|_| "Tor bootstrap timed out after 120 seconds")?;

        let addr = onion_address.lock().await.clone()
            .ok_or_else(|| "Could not determine .onion address".to_string())?;

        println!("🧅 Tor hidden service running at http://{}", addr);

        Ok(Self {
            data_dir,
            process: Arc::new(Mutex::new(Some(child))),
            onion_address,
            socks_port: Arc::new(Mutex::new(socks_port)),
        })
    }

    pub async fn onion_address(&self) -> Option<String> {
        self.onion_address.lock().await.clone()
    }

    pub async fn socks_port(&self) -> u16 {
        *self.socks_port.lock().await
    }

    pub async fn shutdown(&self) {
        let mut guard = self.process.lock().await;
        if let Some(mut child) = guard.take() {
            child.kill().await.ok();
            child.wait().await.ok();
            println!("🧅 Tor process stopped");
        }
    }
}
