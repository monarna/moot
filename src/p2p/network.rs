use libp2p::{
    core::transport::{DialOpts, ListenerId, Transport, TransportError, TransportEvent},
    swarm::{NetworkBehaviour, Swarm, SwarmEvent},
    identity, PeerId, Multiaddr,
    gossipsub::{self, MessageAuthenticity, ValidationMode},
    kad::{self, store::MemoryStore},
    identify,
    ping,
    noise, yamux, tcp,
};
use libp2p::tcp::tokio::TcpStream as Libp2pTcpStream;
use tokio::sync::{mpsc, broadcast, oneshot};
use std::time::Duration;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::net::SocketAddr;
use serde::{Serialize, Deserialize};
use crate::models::{Leaf, Branch, Root, Report, BlacklistVote};
use chrono::Utc;
use sha2::Digest;
use futures::StreamExt;
use futures::future::BoxFuture;

const GOSSIPSUB_TOPIC: &str = "moot/0.1.0";
const BOOTSTRAP_INTERVAL_SECS: u64 = 300;

/// Bootstrap nodes for initial peer discovery.
/// Replace these with well-known .onion addresses in production.
/// Format: /ip4/<ip>/tcp/<port>/p2p/<peer_id> or direct addresses.
pub const BOOTSTRAP_NODES: &[&str] = &[
    // TODO: Add real bootstrap nodes
    // "/dns4/seed1.moot.net/tcp/9000/p2p/<peer_id>",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2PMessage {
    NewLeaf(Leaf),
    NewBranch(Branch),
    NewRoot(Root),
    Report(Report),
    BlacklistVote(BlacklistVote),
    PromoteLegendary(String, String),
}

#[derive(Debug, Clone, Serialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub addresses: Vec<String>,
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
}

pub enum NetworkCommand {
    AddPeer(PeerId, Multiaddr),
    #[allow(dead_code)]
    Bootstrap,
    GetPeers(oneshot::Sender<Vec<PeerInfo>>),
}

/// A Transport that dials through a SOCKS5 proxy (Tor).
/// Does not support listening — use TcpTransport for that.
struct Socks5Transport {
    proxy_addr: SocketAddr,
}

impl Socks5Transport {
    fn new(proxy_addr: SocketAddr) -> Self {
        Self { proxy_addr }
    }
}

impl Transport for Socks5Transport {
    type Output = Libp2pTcpStream;
    type Error = std::io::Error;
    type ListenerUpgrade = futures::future::Ready<Result<Self::Output, Self::Error>>;
    type Dial = BoxFuture<'static, Result<Self::Output, Self::Error>>;

    fn listen_on(
        &mut self,
        _id: ListenerId,
        addr: Multiaddr,
    ) -> Result<(), TransportError<Self::Error>> {
        Err(TransportError::MultiaddrNotSupported(addr))
    }

    fn remove_listener(&mut self, _id: ListenerId) -> bool {
        false
    }

    fn dial(
        &mut self,
        addr: Multiaddr,
        _opts: DialOpts,
    ) -> Result<Self::Dial, TransportError<Self::Error>> {
        let proxy = self.proxy_addr;
        let (host, port) = parse_multiaddr(&addr)
            .map_err(|_| TransportError::MultiaddrNotSupported(addr.clone()))?;

        Ok(Box::pin(async move {
            use tokio_socks::tcp::Socks5Stream;
            let stream = Socks5Stream::connect(proxy, (host.as_str(), port))
                .await
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::ConnectionRefused, e))?;
            Ok(Libp2pTcpStream(stream.into_inner()))
        }))
    }

    fn poll(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<TransportEvent<Self::ListenerUpgrade, Self::Error>> {
        Poll::Pending
    }
}

fn parse_multiaddr(addr: &Multiaddr) -> Result<(String, u16), ()> {
    use libp2p::core::multiaddr::Protocol;
    let mut host = None;
    let mut port = None;
    for p in addr.iter() {
        match p {
            Protocol::Ip4(ip) => host = Some(ip.to_string()),
            Protocol::Ip6(ip) => host = Some(ip.to_string()),
            Protocol::Dns4(dns) | Protocol::Dns6(dns) | Protocol::Dns(dns) => {
                host = Some(dns.to_string())
            }
            Protocol::Tcp(p) => port = Some(p),
            _ => {}
        }
    }
    match (host, port) {
        (Some(h), Some(p)) => Ok((h, p)),
        _ => Err(()),
    }
}

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "MootEvent")]
struct MootBehaviour {
    gossipsub: gossipsub::Behaviour,
    kademlia: kad::Behaviour<MemoryStore>,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

#[derive(Debug)]
enum MootEvent {
    Gossipsub(gossipsub::Event),
    Kademlia(kad::Event),
    Identify(identify::Event),
    #[allow(dead_code)]
    Ping(ping::Event),
}

impl From<gossipsub::Event> for MootEvent {
    fn from(e: gossipsub::Event) -> Self { MootEvent::Gossipsub(e) }
}
impl From<kad::Event> for MootEvent {
    fn from(e: kad::Event) -> Self { MootEvent::Kademlia(e) }
}
impl From<identify::Event> for MootEvent {
    fn from(e: identify::Event) -> Self { MootEvent::Identify(e) }
}
impl From<ping::Event> for MootEvent {
    fn from(e: ping::Event) -> Self { MootEvent::Ping(e) }
}

impl MootBehaviour {
    fn new(local_key: &identity::Keypair) -> Result<Self, Box<dyn std::error::Error>> {
        let peer_id = PeerId::from(local_key.public());

        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .protocol_id_prefix("/moot")
            .validation_mode(ValidationMode::Permissive)
            .message_id_fn(|msg| {
                let hash = sha2::Sha256::digest(&msg.data);
                gossipsub::MessageId::new(&hash[..20])
            })
            .build()?;
        let gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )?;

        let kademlia = kad::Behaviour::new(peer_id, MemoryStore::new(peer_id));

        let identify = identify::Behaviour::new(
            identify::Config::new("/moot/0.1.0".to_string(), local_key.public())
                .with_interval(Duration::from_secs(60)),
        );

        let ping = ping::Behaviour::new(
            ping::Config::new().with_timeout(Duration::from_secs(20))
        );

        Ok(Self { gossipsub, kademlia, identify, ping })
    }
}

#[derive(Clone)]
pub struct P2PNetwork {
    #[allow(dead_code)]
    pub peer_id: PeerId,
    publish_tx: mpsc::Sender<P2PMessage>,
    msg_tx: mpsc::Sender<P2PMessage>,
    #[allow(dead_code)]
    ws_broadcast: broadcast::Sender<String>,
    command_tx: mpsc::Sender<NetworkCommand>,
}

impl P2PNetwork {
    /// Create a new P2P network. If `socks_port` is `Some`, all outbound
    /// libp2p traffic is routed through the Tor SOCKS5 proxy at that port.
    pub fn new(
        ws_broadcast: broadcast::Sender<String>,
        socks_port: Option<u16>,
    ) -> (Self, mpsc::Receiver<P2PMessage>, mpsc::Receiver<P2PMessage>) {
        let (publish_tx, publish_rx) = mpsc::channel::<P2PMessage>(256);
        let (msg_tx, msg_rx) = mpsc::channel::<P2PMessage>(256);
        let (command_tx, command_rx) = mpsc::channel::<NetworkCommand>(64);

        let local_key = identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(local_key.public());

        let network = Self {
            peer_id,
            publish_tx,
            msg_tx: msg_tx.clone(),
            ws_broadcast: ws_broadcast.clone(),
            command_tx,
        };

        let network_for_spawn = network.clone();
        tokio::spawn(async move {
            if let Err(e) = run_swarm(
                local_key,
                publish_rx,
                msg_tx,
                ws_broadcast,
                command_rx,
                socks_port,
            ).await {
                eprintln!("❌ P2P swarm exited: {}", e);
            }
        });

        let (_, second_rx) = mpsc::channel::<P2PMessage>(1);
        (network_for_spawn, second_rx, msg_rx)
    }

    pub fn get_publish_sender(&self) -> mpsc::Sender<P2PMessage> {
        self.publish_tx.clone()
    }

    pub fn get_msg_sender(&self) -> mpsc::Sender<P2PMessage> {
        self.msg_tx.clone()
    }

    pub async fn add_peer(&self, addr: &str) {
        if let Ok(multiaddr) = addr.parse::<Multiaddr>() {
            let peer_id = multiaddr.iter()
                .find_map(|p| if let libp2p::core::multiaddr::Protocol::P2p(h) = p {
                    Some(PeerId::from_multihash(h.into()).ok())
                } else { None })
                .flatten()
                .unwrap_or_else(PeerId::random);
            let _ = self.command_tx.send(NetworkCommand::AddPeer(peer_id, multiaddr)).await;
        }
    }

    pub async fn get_peers(&self) -> Vec<PeerInfo> {
        let (tx, rx) = oneshot::channel();
        if self.command_tx.send(NetworkCommand::GetPeers(tx)).await.is_err() {
            return vec![];
        }
        rx.await.unwrap_or_default()
    }
}

async fn run_swarm(
    local_key: identity::Keypair,
    mut publish_rx: mpsc::Receiver<P2PMessage>,
    msg_tx: mpsc::Sender<P2PMessage>,
    ws_broadcast: broadcast::Sender<String>,
    mut command_rx: mpsc::Receiver<NetworkCommand>,
    socks_port: Option<u16>,
) -> Result<(), Box<dyn std::error::Error>> {
    let peer_id = PeerId::from(local_key.public());
    let behaviour = MootBehaviour::new(&local_key)?;

    let tcp_transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true));

    let transport = if let Some(port) = socks_port {
        let proxy = SocketAddr::from(([127, 0, 0, 1], port));
        let socks = Socks5Transport::new(proxy);
        socks.or_transport(tcp_transport)
            .upgrade(libp2p::core::upgrade::Version::V1)
            .authenticate(noise::Config::new(&local_key)?)
            .multiplex(yamux::Config::default())
            .boxed()
    } else {
        tcp_transport
            .upgrade(libp2p::core::upgrade::Version::V1)
            .authenticate(noise::Config::new(&local_key)?)
            .multiplex(yamux::Config::default())
            .boxed()
    };

    let mut swarm = Swarm::new(
        transport,
        behaviour,
        peer_id,
        libp2p::swarm::Config::with_tokio_executor(),
    );
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;

    let topic = gossipsub::IdentTopic::new(GOSSIPSUB_TOPIC);
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;
    swarm.behaviour_mut().kademlia.set_mode(Some(kad::Mode::Server));

    println!("🧩 libp2p peer ID: {:?}", peer_id);
    println!("📨 Gossipsub topic: {}", GOSSIPSUB_TOPIC);

    let mut bootstrap_timer = tokio::time::interval(Duration::from_secs(BOOTSTRAP_INTERVAL_SECS));
    let mut peers_cache: Vec<PeerInfo> = vec![];

    loop {
        tokio::select! {
            Some(msg) = publish_rx.recv() => {
                let data = serde_json::to_vec(&msg).unwrap_or_default();
                if data.is_empty() { continue; }
                match swarm.behaviour_mut().gossipsub.publish(topic.clone(), data) {
                    Ok(_msg_id) => {
                        if let Ok(serialized) = serde_json::to_string(&msg) {
                            let _ = ws_broadcast.send(serialized);
                        }
                    }
                    Err(e) => eprintln!("❌ Gossipsub publish failed: {}", e),
                }
            }

            Some(cmd) = command_rx.recv() => {
                match cmd {
                    NetworkCommand::AddPeer(pid, addr) => {
                        swarm.behaviour_mut().kademlia.add_address(&pid, addr.clone());
                        let _ = swarm.dial(addr.clone());
                        println!("👥 Added peer: {:?}", pid);
                    }
                    NetworkCommand::Bootstrap => {
                        let _ = swarm.behaviour_mut().kademlia.bootstrap();
                    }
                    NetworkCommand::GetPeers(tx) => {
                        let _ = tx.send(peers_cache.clone());
                    }
                }
            }

            event = swarm.next() => {
                let Some(event) = event else { break; };
                match event {
                    SwarmEvent::Behaviour(bev) => {
                        match bev {
                            MootEvent::Gossipsub(gossipsub::Event::Message { message, .. }) => {
                                if let Ok(p2p_msg) = serde_json::from_slice::<P2PMessage>(&message.data) {
                                    if let Ok(serialized) = serde_json::to_string(&p2p_msg) {
                                        let _ = ws_broadcast.send(serialized);
                                    }
                                    let _ = msg_tx.send(p2p_msg).await;
                                }
                            }
                            MootEvent::Gossipsub(_) => {}
                            MootEvent::Kademlia(kad::Event::RoutingUpdated { peer, .. }) => {
                                if !peers_cache.iter().any(|p| p.peer_id == peer.to_string()) {
                                    peers_cache.push(PeerInfo {
                                        peer_id: peer.to_string(),
                                        addresses: vec![],
                                        last_seen: Some(Utc::now()),
                                    });
                                }
                            }
                            MootEvent::Kademlia(_) => {}
                            MootEvent::Identify(identify::Event::Received { peer_id, info, .. }) => {
                                let peer_str = peer_id.to_string();
                                let addrs: Vec<String> = info.listen_addrs.iter().map(|a| a.to_string()).collect();
                                if let Some(entry) = peers_cache.iter_mut().find(|p| p.peer_id == peer_str) {
                                    entry.addresses = addrs;
                                    entry.last_seen = Some(Utc::now());
                                } else {
                                    peers_cache.push(PeerInfo {
                                        peer_id: peer_str,
                                        addresses: addrs,
                                        last_seen: Some(Utc::now()),
                                    });
                                }
                            }
                            MootEvent::Identify(_) => {}
                            MootEvent::Ping(_) => {}
                        }
                    }
                    SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                        println!("🔗 Connected to {:?} via {}", peer_id, endpoint.get_remote_address());
                        let _ = swarm.behaviour_mut().kademlia.bootstrap();
                    }
                    SwarmEvent::ConnectionClosed { peer_id, .. } => {
                        println!("🔌 Disconnected from {:?}", peer_id);
                    }
                    SwarmEvent::NewListenAddr { address, .. } => {
                        println!("🎧 Listening on: {}", address);
                    }
                    _ => {}
                }
            }

            _ = bootstrap_timer.tick() => {
                let _ = swarm.behaviour_mut().kademlia.bootstrap();
            }
        }
    }

    Ok(())
}
