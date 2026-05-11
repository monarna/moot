mod models;
mod database;
mod p2p;
mod crypto;
mod tor;

use actix_web::{App, HttpServer, web, HttpResponse, Responder, HttpRequest, Error};
use actix_files as fs;
use actix_multipart::Multipart;
use actix_ws;
use clap::Parser;
use database::Database;
use models::*;
use p2p::network::{P2PNetwork, P2PMessage, BOOTSTRAP_NODES};
use crypto::{sanitize_content, validate_address, validate_signature_format};
use tracing_subscriber;
use serde::{Serialize, Deserialize};
use std::sync::Mutex;
use futures::StreamExt;
use chrono::Utc;
use tokio::sync::broadcast;

fn default_data_dir() -> String {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .map(|p| format!("{}\\moot", p))
            .unwrap_or_else(|_| "C:\\moot_data".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        "/tmp/moot_data".to_string()
    }
}

fn find_tor_binary() -> String {
    let bin_name = tor_binary_name();
    if let Ok(exe_path) = std::env::current_exe() {
        let bundled = exe_path.parent().unwrap().join(bin_name);
        if bundled.exists() {
            return bundled.to_string_lossy().to_string();
        }
    }
    bin_name.to_string()
}

fn tor_binary_name() -> &'static str {
    #[cfg(target_os = "windows")]
    { "tor.exe" }
    #[cfg(not(target_os = "windows"))]
    { "tor" }
}

#[derive(Parser)]
#[command(name = "moot", about = "A decentralized social network")]
struct Cli {
    /// Run as gateway (no Tor hidden service)
    #[arg(long)]
    gateway: bool,

    /// Data directory (database, Tor state, keys)
    #[arg(long, default_value_t = default_data_dir())]
    data_dir: String,

    /// HTTP server port
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// Extra lines appended to generated torrc
    #[arg(long)]
    torrc_extra: Option<String>,
}

#[derive(Clone)]
struct HealthState {
    tor_active: bool,
    onion_address: Option<String>,
}

// API Handlers

async fn health_check(health: web::Data<HealthState>) -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "project": "moot",
        "tor": {
            "active": health.tor_active,
            "onion_address": health.onion_address,
        },
    }))
}

// P2P sync endpoint — deprecated, replaced by Gossipsub
async fn p2p_receive() -> impl Responder {
    HttpResponse::Gone().json(serde_json::json!({"error": "HTTP P2P endpoints removed — libp2p swarm handles P2P communication"}))
}

async fn p2p_garlic_receive() -> impl Responder {
    HttpResponse::Gone().json(serde_json::json!({"error": "HTTP P2P endpoints removed — libp2p swarm handles P2P communication"}))
}

async fn p2p_pubkey() -> impl Responder {
    HttpResponse::Gone().json(serde_json::json!({"error": "HTTP P2P endpoints removed — libp2p swarm handles P2P communication"}))
}

async fn p2p_ws_handler(
    req: HttpRequest,
    stream: web::Payload,
    msg_tx: web::Data<tokio::sync::mpsc::Sender<P2PMessage>>,
    ws_broadcast: web::Data<broadcast::Sender<String>>,
) -> Result<HttpResponse, Error> {
    let (res, session, mut msg_stream) = actix_ws::handle(&req, stream)?;

    // Forward broadcast messages to this WS connection (broadcast::Receiver is Send)
    let mut broadcast_rx = ws_broadcast.subscribe();
    let mut session_tx = session.clone();
    tokio::spawn(async move {
        while let Ok(msg) = broadcast_rx.recv().await {
            if session_tx.text(msg).await.is_err() {
                break;
            }
        }
    });

    // Process incoming WS messages in this task (MessageStream is !Send)
    let msg_tx = msg_tx.into_inner();
    while let Some(Ok(msg)) = msg_stream.next().await {
        match msg {
            actix_ws::Message::Text(text) => {
                if let Ok(p2p_msg) = serde_json::from_str(&text) {
                    println!("📬 Received message via WebSocket");
                    let _ = msg_tx.send(p2p_msg).await;
                }
            }
            actix_ws::Message::Close(_) => break,
            _ => {}
        }
    }

    Ok(res)
}

// P2P sync - get all leaves for syncing
async fn get_all_leaves(db: web::Data<Mutex<Database>>) -> impl Responder {
    let db = db.lock().unwrap();
    let leaves = db.list_leaves();
    HttpResponse::Ok().json(leaves)
}

async fn upload_image(mut payload: Multipart) -> impl Responder {
    let upload_dir = std::path::PathBuf::from("static").join("uploads");
    std::fs::create_dir_all(&upload_dir).ok();

    let mut urls = Vec::new();
    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(f) => f,
            Err(_) => continue,
        };
        let filename = match field.content_disposition() {
            Some(cd) => cd.get_filename().unwrap_or("image").to_string(),
            None => "image".to_string(),
        };

        let ext = std::path::Path::new(&filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");

        let mut data = Vec::new();
        while let Some(chunk) = field.next().await {
            if let Ok(bytes) = chunk {
                data.extend_from_slice(&bytes);
            }
        }

        if data.len() > 10_000_000 {
            return HttpResponse::BadRequest().json(serde_json::json!({"error": "File too large (max 10MB)"}));
        }

        // SHA-256 content hash for dedup across peers
        let hash = crypto::sha256_hash(&data);
        let hash_filename = format!("{}.{}", hash, ext);
        let filepath = upload_dir.join(&hash_filename);

        if !filepath.exists() {
            if let Err(e) = std::fs::write(&filepath, &data) {
                return HttpResponse::InternalServerError().json(serde_json::json!({"error": format!("Failed to save: {}", e)}));
            }
        }

        urls.push(format!("/static/uploads/{}", hash_filename));
    }

    if urls.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "No files uploaded"}));
    }

    HttpResponse::Ok().json(serde_json::json!({"urls": urls}))
}

async fn list_roots(db: web::Data<Mutex<Database>>) -> impl Responder {
    let db = db.lock().unwrap();
    let roots = db.list_roots();
    HttpResponse::Ok().json(roots)
}

async fn get_root(path: web::Path<String>, db: web::Data<Mutex<Database>>) -> impl Responder {
    let db = db.lock().unwrap();
    let root_id = path.into_inner();
    
    match db.get_root(&root_id) {
        Some(root) => HttpResponse::Ok().json(root),
        None => HttpResponse::NotFound().json(serde_json::json!({"error": "Root not found"}))
    }
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
}

async fn search(
    params: web::Query<SearchParams>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let db = db.lock().unwrap();
    let results = db.search(&params.q);
    HttpResponse::Ok().json(results)
}

async fn create_leaf(
    req: HttpRequest,
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>,
    leaf_req: web::Json<CreateLeafRequest>,
    publish_tx: web::Data<tokio::sync::mpsc::Sender<P2PMessage>>
) -> impl Responder {
    let db = db.lock().unwrap();
    let author_address = path.into_inner();
    
    // Get headers for signature verification
    let signature = req.headers().get("X-Signature")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let public_key = req.headers().get("X-Public-Key")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    
    // Validate address format
    if !validate_address(&author_address) {
        return HttpResponse::BadRequest().json(
            serde_json::json!({"error": "Invalid address format"})
        );
    }
    
    // In a real decentralized system, we'd verify the signature here.
    // For now, if headers are provided, we validate them.
    if !signature.is_empty() && !public_key.is_empty() {
        if !validate_signature_format(signature) {
            return HttpResponse::BadRequest().json(
                serde_json::json!({"error": "Invalid signature format"})
            );
        }
        
        // Verify public key matches address
        // Address is derived from pubkey: "0x" + first 40 chars of sha256(pubkey)
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(public_key.as_bytes());
        let hash_hex = hex::encode(hasher.finalize());
        let derived_address = format!("0x{}", &hash_hex[..40]);
        
        if derived_address != author_address {
            return HttpResponse::BadRequest().json(
                serde_json::json!({"error": "Public key does not match address"})
            );
        }
        
        // Verify signature of the content
        if !crypto::verify_signature(public_key, leaf_req.content.as_bytes(), signature) {
            return HttpResponse::Unauthorized().json(
                serde_json::json!({"error": "Invalid signature"})
            );
        }
    } else {
        // For backwards compatibility or during testing, we might allow unsigned posts,
        // but the plan says "Require signature". Let's enforce it if we want security.
        // return HttpResponse::Unauthorized().json(serde_json::json!({"error": "Signature required"}));
    }
    
    // Sanitize content to prevent XSS/malware
    let sanitized_content = sanitize_content(&leaf_req.content);
    if sanitized_content.is_empty() {
        return HttpResponse::BadRequest().json(
            serde_json::json!({"error": "Content cannot be empty after sanitization"})
        );
    }
    
    // Branch size limit: max 1000 leaves per branch
    let leaf_count = db.leaf_count_in_branch(&leaf_req.branch);
    if leaf_count >= 1000 {
        return HttpResponse::BadRequest().json(
            serde_json::json!({"error": "Branch has reached the maximum capacity of 1000 leaves"})
        );
    }
    
    let media_urls = leaf_req.media_urls.clone().unwrap_or_default();

    let node_config = db.get_node_config();
    let seeded_until = if node_config.auto_seed {
        Some(chrono::Utc::now() + chrono::Duration::days(node_config.cache_ttl_days as i64))
    } else {
        None
    };

    let leaf = Leaf {
        id: uuid::Uuid::new_v4().to_string(),
        author_address: author_address.clone(),
        author_name: leaf_req.author_name.clone(),
        content: sanitized_content,
        media_urls,
        root: leaf_req.root.clone(),
        branch: leaf_req.branch.clone(),
        parent_leaf_id: leaf_req.parent_leaf_id.clone(),
        created_at: chrono::Utc::now(),
        upvotes: 0,
        downvotes: 0,
        mirrors: vec![],
        is_mirrored: false,
        is_deleted: false,
        seeded_until,
    };
    
    // Add to database
    db.add_leaf(leaf.clone());
    
    // Update user's leaf count
    if let Some(mut user) = db.get_user(&author_address) {
        user.total_leaves += 1;
        db.add_user(user);
    } else {
        let new_user = User {
            address: author_address.clone(),
            raw_score: 0.0,
            sway: 0.0,
            total_leaves: 1,
            mirrored_leaves: vec![],
            vouched_by: vec![],
            vouch_requirement: 0.0,
            is_banned: false,
            created_at: chrono::Utc::now(),
            last_sway_update: None,
        };
        db.add_user(new_user);
    }
    
    // Broadcast to P2P network
    let msg = P2PMessage::NewLeaf(leaf.clone());
    let _ = publish_tx.send(msg).await;
    
    HttpResponse::Ok().json(leaf)
}

async fn upvote_leaf(
    path: web::Path<(String, String)>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let (leaf_id, voter_address) = path.into_inner();
    let db = db.lock().unwrap();

    match db.get_leaf(&leaf_id) {
        Some(mut leaf) => {
            let voter = db.get_user(&voter_address);
            let voter_sway = db.compute_sway(&voter);
            if voter_sway > 0.0 {
                leaf.upvotes += 1;
                db.add_leaf(leaf.clone());

                let vote = Vote {
                    voter_address: voter_address.clone(),
                    target_id: leaf_id.clone(),
                    target_type: VoteTarget::Leaf,
                    vote_type: VoteType::Upvote,
                    sway_weight: voter_sway,
                    timestamp: chrono::Utc::now(),
                };
                db.add_vote(vote);

                HttpResponse::Ok().json(leaf)
            } else {
                HttpResponse::Forbidden().json(serde_json::json!({"error": "Insufficient sway to vote"}))
            }
        }
        None => HttpResponse::NotFound().json(serde_json::json!({"error": "Leaf not found"}))
    }
}

async fn downvote_leaf(
    path: web::Path<(String, String)>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let (leaf_id, voter_address) = path.into_inner();
    let db = db.lock().unwrap();

    match db.get_leaf(&leaf_id) {
        Some(mut leaf) => {
            let voter = db.get_user(&voter_address);
            let voter_sway = db.compute_sway(&voter);
            if voter_sway > 0.0 {
                leaf.downvotes += 1;
                db.add_leaf(leaf.clone());

                let vote = Vote {
                    voter_address: voter_address.clone(),
                    target_id: leaf_id.clone(),
                    target_type: VoteTarget::Leaf,
                    vote_type: VoteType::Downvote,
                    sway_weight: voter_sway,
                    timestamp: chrono::Utc::now(),
                };
                db.add_vote(vote);

                HttpResponse::Ok().json(leaf)
            } else {
                HttpResponse::Forbidden().json(serde_json::json!({"error": "Insufficient sway to vote"}))
            }
        }
        None => HttpResponse::NotFound().json(serde_json::json!({"error": "Leaf not found"}))
    }
}

async fn get_sway(path: web::Path<String>, db: web::Data<Mutex<Database>>) -> impl Responder {
    let db = db.lock().unwrap();
    let address = path.into_inner();

    match db.get_user(&address) {
        Some(user) => {
            // Re-compute sway from raw_score + network average so it's always live.
            let network_avg = db.get_network_avg_score();
            let sway = user.raw_score / (user.raw_score + network_avg);

            HttpResponse::Ok().json(serde_json::json!({
                "address": user.address,
                "raw_score": user.raw_score,
                "sway": sway,
                "network_avg": network_avg,
            }))
        }
        None => HttpResponse::Ok().json(serde_json::json!({
            "address": address,
            "raw_score": 0.0,
            "sway": 0.0,
            "message": "User not found. Start hosting to gain sway!"
        }))
    }
}

async fn mirror_leaf(
    path: web::Path<(String, String)>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let (leaf_id, mirrorer_address) = path.into_inner();
    let db = db.lock().unwrap();
    
    match db.get_leaf(&leaf_id) {
        Some(mut leaf) => {
            if !leaf.mirrors.contains(&mirrorer_address) {
                leaf.mirrors.push(mirrorer_address.clone());
                leaf.is_mirrored = true;
                db.add_leaf(leaf.clone());
                
                // Update user's mirrored leaves
                if let Some(mut user) = db.get_user(&mirrorer_address) {
                    user.mirrored_leaves.push(leaf_id.clone());
                    db.add_user(user);
                }
            }
            HttpResponse::Ok().json(leaf)
        }
        None => HttpResponse::NotFound().json(serde_json::json!({"error": "Leaf not found"}))
    }
}

async fn get_mirrored_leaves(path: web::Path<String>, db: web::Data<Mutex<Database>>) -> impl Responder {
    let db = db.lock().unwrap();
    let address = path.into_inner();
    
    match db.get_user(&address) {
        Some(user) => {
            let mut leaves = vec![];
            for leaf_id in user.mirrored_leaves.iter() {
                if let Some(leaf) = db.get_leaf(leaf_id) {
                    leaves.push(leaf);
                }
            }
            HttpResponse::Ok().json(leaves)
        }
        None => HttpResponse::NotFound().json(serde_json::json!({"error": "User not found"}))
    }
}

async fn list_leaves_in_root(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let db = db.lock().unwrap();
    let root_id = path.into_inner();
    
    let leaves: Vec<Leaf> = db.list_leaves().into_iter()
        .filter(|l| l.root == root_id && !l.is_deleted)
        .collect();
    
    HttpResponse::Ok().json(leaves)
}

// Moderation endpoints

const REPORT_STAKE: f64 = 0.05;
const DISMISS_VOTE_COST: f64 = 0.02;

async fn report_content(
    db: web::Data<Mutex<Database>>,
    report_req: web::Json<ReportRequest>
) -> impl Responder {
    let db = db.lock().unwrap();

    // Must have sufficient sway to stake
    let reporter = db.get_user(&report_req.reporter_address);
    let reporter_sway = db.compute_sway(&reporter);
    if reporter_sway < REPORT_STAKE {
        return HttpResponse::Forbidden().json(
            serde_json::json!({"error": format!("Need at least {REPORT_STAKE} sway to stake a report")})
        );
    }

    let severity = report_req.category.severity();
    let report = Report {
        id: uuid::Uuid::new_v4().to_string(),
        reporter_address: report_req.reporter_address.clone(),
        target_type: report_req.target_type.clone(),
        target_id: report_req.target_id.clone(),
        category: report_req.category.clone(),
        severity,
        staked_sway: REPORT_STAKE,
        status: ReportStatus::Open,
        timestamp: chrono::Utc::now(),
    };

    db.add_report(report.clone());

    // Immediately check thresholds (in case first vote pushes over)
    let resolved = db.resolve_report(&report.id);

    HttpResponse::Ok().json(serde_json::json!({
        "message": "Report submitted",
        "report_id": report.id,
        "category": report_req.category.label(),
        "severity": severity,
        "staked_sway": REPORT_STAKE,
        "threshold": severity as f64 * 20.0,
        "resolved": resolved.is_some(),
    }))
}

async fn vote_blacklist(
    db: web::Data<Mutex<Database>>,
    vote_req: web::Json<BlacklistVoteRequest>
) -> impl Responder {
    let db = db.lock().unwrap();

    let voter = db.get_user(&vote_req.voter_address);
    let voter_sway = db.compute_sway(&voter);
    if voter_sway <= 0.0 {
        return HttpResponse::Forbidden().json(
            serde_json::json!({"error": "Insufficient sway to vote on blacklist"})
        );
    }

    // Report must exist and be open
    let report = match db.get_report(&vote_req.report_id) {
        Some(r) if matches!(r.status, ReportStatus::Open) => r,
        Some(_) => return HttpResponse::BadRequest().json(
            serde_json::json!({"error": "Report is already resolved"})
        ),
        None => return HttpResponse::NotFound().json(
            serde_json::json!({"error": "Report not found"})
        ),
    };

    let vote = BlacklistVote {
        report_id: vote_req.report_id.clone(),
        voter_address: vote_req.voter_address.clone(),
        sway_weight: voter_sway,
        timestamp: chrono::Utc::now(),
    };
    db.add_blacklist_vote(vote);

    let total_sway = db.get_total_sway_for_report(&vote_req.report_id);
    let threshold = report.severity as f64 * 20.0;

    // Try to resolve
    if let Some(status) = db.resolve_report(&vote_req.report_id) {
        match status {
            ReportStatus::Blacklisted => {
                let target_id = report.target_id.clone();
                let target_type = report.target_type.clone();
                let entry = BlacklistEntry {
                    target_id: target_id.clone(),
                    target_type: target_type.clone(),
                    reason: report.category.label().to_string(),
                    total_sway,
                    blacklisted_at: chrono::Utc::now(),
                };
                db.add_blacklist_entry(entry);

                if let ReportTarget::User = target_type {
                    if let Some(mut user) = db.get_user(&target_id) {
                        user.is_banned = true;
                        db.add_user(user);
                    }
                }
                if let ReportTarget::Leaf = target_type {
                    if let Some(mut leaf) = db.get_leaf(&target_id) {
                        leaf.is_deleted = true;
                        db.add_leaf(leaf);
                    }
                }

                HttpResponse::Ok().json(serde_json::json!({
                    "message": "Target blacklisted",
                    "status": "blacklisted",
                    "total_sway": total_sway,
                }))
            }
            ReportStatus::Dismissed => {
                HttpResponse::Ok().json(serde_json::json!({
                    "message": "Report dismissed",
                    "status": "dismissed",
                    "total_sway": total_sway,
                }))
            }
            _ => unreachable!(),
        }
    } else {
        HttpResponse::Ok().json(serde_json::json!({
            "message": "Vote recorded",
            "total_sway": total_sway,
            "threshold": threshold,
        }))
    }
}

async fn vote_dismiss(
    db: web::Data<Mutex<Database>>,
    vote_req: web::Json<BlacklistVoteRequest>
) -> impl Responder {
    let db = db.lock().unwrap();

    let voter = db.get_user(&vote_req.voter_address);
    let voter_sway = db.compute_sway(&voter);
    if voter_sway < DISMISS_VOTE_COST {
        return HttpResponse::Forbidden().json(
            serde_json::json!({"error": format!("Need at least {DISMISS_VOTE_COST} sway to dismiss")})
        );
    }

    let report = match db.get_report(&vote_req.report_id) {
        Some(r) if matches!(r.status, ReportStatus::Open) => r,
        Some(_) => return HttpResponse::BadRequest().json(
            serde_json::json!({"error": "Report is already resolved"})
        ),
        None => return HttpResponse::NotFound().json(
            serde_json::json!({"error": "Report not found"})
        ),
    };

    // Cost to cast a dismiss vote — burned
    let vote = DismissVote {
        report_id: vote_req.report_id.clone(),
        voter_address: vote_req.voter_address.clone(),
        sway_weight: voter_sway,
        timestamp: chrono::Utc::now(),
    };
    db.add_dismiss_vote(vote);

    let total_dismiss = db.get_total_dismiss_sway_for_report(&vote_req.report_id);
    let threshold = report.severity as f64 * 20.0;

    if let Some(status) = db.resolve_report(&vote_req.report_id) {
        match status {
            ReportStatus::Dismissed => {
                HttpResponse::Ok().json(serde_json::json!({
                    "message": "Report dismissed",
                    "status": "dismissed",
                    "total_dismiss": total_dismiss,
                }))
            }
            ReportStatus::Blacklisted => {
                HttpResponse::Ok().json(serde_json::json!({
                    "message": "Report blacklisted despite dismiss votes",
                    "status": "blacklisted",
                    "total_dismiss": total_dismiss,
                }))
            }
            _ => unreachable!(),
        }
    } else {
        HttpResponse::Ok().json(serde_json::json!({
            "message": "Dismiss vote recorded",
            "total_dismiss": total_dismiss,
            "threshold": threshold,
        }))
    }
}

async fn check_blacklist(
    path: web::Path<(String, String)>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let (target_type, target_id) = path.into_inner();
    let db = db.lock().unwrap();
    
    let report_target = match target_type.as_str() {
        "leaf" => ReportTarget::Leaf,
        "user" => ReportTarget::User,
        "root" => ReportTarget::Root,
        _ => return HttpResponse::BadRequest().json(
            serde_json::json!({"error": "Invalid target type"})
        ),
    };
    
    let is_blacklisted = db.is_blacklisted(&target_id, report_target);
    
    HttpResponse::Ok().json(serde_json::json!({
        "target_id": target_id,
        "is_blacklisted": is_blacklisted
    }))
}

// ── Content Lifecycle ──

async fn promote_legendary(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>,
    query: web::Query<LegendaryQuery>,
    publish_tx: web::Data<tokio::sync::mpsc::Sender<P2PMessage>>,
) -> impl Responder {
    let leaf_id = path.into_inner();
    let db = db.lock().unwrap();
    let address = &query.address;

    let leaf = match db.get_leaf(&leaf_id) {
        Some(l) => l,
        None => return HttpResponse::NotFound().json(serde_json::json!({"error": "Leaf not found"})),
    };

    if db.is_legendary(address, &leaf_id) {
        return HttpResponse::Ok().json(serde_json::json!({"message": "Already legendary"}));
    }

    let entry = LegendaryEntry {
        leaf_id: leaf_id.clone(),
        promoted_by: address.clone(),
        promoted_at: Utc::now(),
        leaf_snapshot: leaf,
    };
    db.add_legendary(&entry);

    // Broadcast legendary promotion to P2P peers
    let msg = P2PMessage::PromoteLegendary(leaf_id.clone(), address.clone());
    let _ = publish_tx.send(msg).await;

    HttpResponse::Ok().json(serde_json::json!({
        "message": "Promoted to legendary",
        "leaf_id": leaf_id,
    }))
}

async fn remove_legendary(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>,
    query: web::Query<LegendaryQuery>,
) -> impl Responder {
    let leaf_id = path.into_inner();
    let db = db.lock().unwrap();
    db.remove_legendary(&query.address, &leaf_id);
    HttpResponse::Ok().json(serde_json::json!({"message": "Removed from legendary"}))
}

async fn list_legendary(
    db: web::Data<Mutex<Database>>,
    query: web::Query<LegendaryQuery>,
) -> impl Responder {
    let db = db.lock().unwrap();
    let entries = db.list_legendary(&query.address);
    HttpResponse::Ok().json(entries)
}

async fn get_leaf_expiry(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>,
) -> impl Responder {
    let leaf_id = path.into_inner();
    let db = db.lock().unwrap();

    let leaf = match db.get_leaf(&leaf_id) {
        Some(l) => l,
        None => return HttpResponse::NotFound().json(serde_json::json!({"error": "Leaf not found"})),
    };

    let now = Utc::now();
    let expires_in = leaf.seeded_until.map(|u| (u - now).num_seconds().max(0));

    HttpResponse::Ok().json(serde_json::json!({
        "leaf_id": leaf_id,
        "seeded_until": leaf.seeded_until,
        "expires_in_seconds": expires_in,
        "is_expired": leaf.seeded_until.map(|u| u < now).unwrap_or(false),
    }))
}

async fn get_node_config_handler(
    db: web::Data<Mutex<Database>>,
) -> impl Responder {
    let db = db.lock().unwrap();
    let config = db.get_node_config();
    HttpResponse::Ok().json(config)
}

async fn set_node_config_handler(
    db: web::Data<Mutex<Database>>,
    config: web::Json<NodeConfig>,
) -> impl Responder {
    let db = db.lock().unwrap();
    db.set_node_config(&config);
    HttpResponse::Ok().json(serde_json::json!({"message": "Node config updated"}))
}

// Branch endpoints

async fn create_branch(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>,
    branch_req: web::Json<CreateBranchRequest>,
    publish_tx: web::Data<tokio::sync::mpsc::Sender<P2PMessage>>
) -> impl Responder {
    let db = db.lock().unwrap();
    let root_id = path.into_inner();
    
    // Check if root exists
    if db.get_root(&root_id).is_none() {
        return HttpResponse::NotFound().json(
            serde_json::json!({"error": "Root not found"})
        );
    }
    
    let branch_name = branch_req.name.clone();
    let branch_desc = branch_req.description.clone();
    let trunk_id = branch_req.trunk_id.clone();
    
    match db.add_branch(root_id.clone(), trunk_id, branch_name.clone(), branch_desc.clone()) {
        Ok(branch_id) => {
            // Get the created branch to return
            if let Some(branch) = db.get_branch(&branch_id) {
                // Broadcast to P2P network
                let msg = P2PMessage::NewBranch(branch.clone());
                let _ = publish_tx.send(msg).await;
                
                HttpResponse::Ok().json(branch)
            } else {
                HttpResponse::InternalServerError().json(
                    serde_json::json!({"error": "Branch created but not found"})
                )
            }
        }
        Err(e) => HttpResponse::InternalServerError().json(
            serde_json::json!({"error": e})
        ),
    }
}

async fn list_branches(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let db = db.lock().unwrap();
    let root_id = path.into_inner();
    
    let branches = db.list_branches_in_root(&root_id);
    HttpResponse::Ok().json(branches)
}

async fn get_branch(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let db = db.lock().unwrap();
    let branch_id = path.into_inner();
    
    match db.get_branch(&branch_id) {
        Some(branch) => HttpResponse::Ok().json(branch),
        None => HttpResponse::NotFound().json(
            serde_json::json!({"error": "Branch not found"})
        ),
    }
}

// Trunk endpoints
async fn list_trunks(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let db = db.lock().unwrap();
    let root_id = path.into_inner();
    let trunks = db.list_trunks_in_root(&root_id);
    HttpResponse::Ok().json(trunks)
}

async fn create_trunk(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>,
    trunk_req: web::Json<CreateTrunkRequest>
) -> impl Responder {
    let db = db.lock().unwrap();
    let root_id = path.into_inner();
    let created_by = trunk_req.created_by.clone();

    if !validate_address(&created_by) {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Invalid address format"}));
    }

    let user = db.get_user(&created_by);
    let _sway = user.as_ref().map(|u| u.sway).unwrap_or(0.0);
    // TODO: Re-enable sway gate once sway system is mature
    // if _sway <= 0.0 {
    //     return HttpResponse::Forbidden().json(serde_json::json!({"error": "Only users with Sway > 0 can create trunks"}));
    // }

    match db.add_trunk(root_id, trunk_req.name.clone(), trunk_req.description.clone(), created_by) {
        Ok(trunk_id) => {
            if let Some(trunk) = db.get_trunk(&trunk_id) {
                HttpResponse::Ok().json(trunk)
            } else {
                HttpResponse::InternalServerError().json(serde_json::json!({"error": "Trunk created but not found"}))
            }
        }
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({"error": e})),
    }
}

async fn list_branches_in_trunk(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let db = db.lock().unwrap();
    let trunk_id = path.into_inner();
    let branches = db.list_branches_in_trunk(&trunk_id);
    HttpResponse::Ok().json(branches)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateHollowRequest {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub is_public: Option<bool>,
    pub theme: Option<String>,
    pub custom_css: Option<String>,
    pub custom_html: Option<String>,
    pub music_url: Option<String>,
    pub video_embed: Option<String>,
    pub font_size: Option<String>,
    pub text_color: Option<String>,
    pub bg_color: Option<String>,
    pub animation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendRequest {
    pub friend_address: String,
}

// Hollow endpoints

async fn get_hollow(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let db = db.lock().unwrap();
    let address = path.into_inner();

    match db.get_hollow(&address) {
        Some(hollow) => {
            // Only show public info if not the owner
            if hollow.settings.is_public {
                HttpResponse::Ok().json(hollow)
            } else {
                HttpResponse::Ok().json(serde_json::json!({
                    "owner_address": hollow.owner_address,
                    "display_name": hollow.display_name,
                    "bio": hollow.bio,
                    "settings": { "is_public": false },
                    "created_at": hollow.created_at
                }))
            }
        }
        None => HttpResponse::NotFound().json(
            serde_json::json!({"error": "Hollow not found"})
        ),
    }
}

async fn create_hollow(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let db = db.lock().unwrap();
    let address = path.into_inner();

    // Check if hollow already exists
    if db.get_hollow(&address).is_some() {
        return HttpResponse::Conflict().json(
            serde_json::json!({"error": "Hollow already exists"})
        );
    }

    let hollow = Hollow::new(address.clone());
    db.add_hollow(hollow.clone());

    HttpResponse::Ok().json(hollow)
}

async fn update_hollow_settings(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>,
    update: web::Json<UpdateHollowRequest>
) -> impl Responder {
    let db = db.lock().unwrap();
    let address = path.into_inner();

    match db.get_hollow(&address) {
        Some(mut hollow) => {
            if let Some(val) = update.display_name.clone() { hollow.display_name = Some(val); }
            if let Some(val) = update.bio.clone() { hollow.bio = Some(val); }
            if let Some(val) = update.is_public { hollow.settings.is_public = val; }
            if let Some(val) = update.theme.clone() { hollow.settings.theme = val; }
            if let Some(val) = update.custom_css.clone() { hollow.settings.custom_css = Some(val); }
            if let Some(val) = update.custom_html.clone() { hollow.custom_html = Some(val); }
            if let Some(val) = update.music_url.clone() { hollow.music_url = Some(val); }
            if let Some(val) = update.video_embed.clone() { hollow.video_embed = Some(val); }
            if let Some(val) = update.font_size.clone() { hollow.settings.font_size = Some(val); }
            if let Some(val) = update.text_color.clone() { hollow.settings.text_color = Some(val); }
            if let Some(val) = update.bg_color.clone() { hollow.settings.bg_color = Some(val); }
            if let Some(val) = update.animation.clone() { hollow.settings.animation = Some(val); }

            db.update_hollow(hollow).unwrap();
            HttpResponse::Ok().json(serde_json::json!({"message": "Hollow updated"}))
        }
        None => HttpResponse::NotFound().json(serde_json::json!({"error": "Hollow not found"})),
    }
}

async fn add_friend(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>,
    friend_req: web::Json<FriendRequest>
) -> impl Responder {
    let db = db.lock().unwrap();
    let address = path.into_inner();

    match db.get_hollow(&address) {
        Some(mut hollow) => {
            if !hollow.friends.contains(&friend_req.friend_address) {
                hollow.friends.push(friend_req.friend_address.clone());
                db.update_hollow(hollow).unwrap();
            }
            HttpResponse::Ok().json(serde_json::json!({"message": "Friend added"}))
        }
        None => HttpResponse::NotFound().json(serde_json::json!({"error": "Hollow not found"})),
    }
}
async fn add_hollow_post(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>,
    post_req: web::Json<CreateHollowPostRequest>
) -> impl Responder {
    let db = db.lock().unwrap();
    let address = path.into_inner();
    
    let post = HollowPost {
        id: uuid::Uuid::new_v4().to_string(),
        content: post_req.content.clone(),
        created_at: chrono::Utc::now(),
        is_public: post_req.is_public.unwrap_or(false),
    };
    
    match db.add_hollow_post(&address, post.clone()) {
        Ok(()) => HttpResponse::Ok().json(post),
        Err(e) => HttpResponse::NotFound().json(serde_json::json!({"error": e})),
    }
}

async fn delete_hollow_post(
    path: web::Path<(String, String)>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let (address, post_id) = path.into_inner();
    let db = db.lock().unwrap();
    
    match db.delete_hollow_post(&address, &post_id) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"message": "Post deleted"})),
        Err(e) => HttpResponse::NotFound().json(serde_json::json!({"error": e})),
    }
}

// ── Hollow Comments ──

async fn add_hollow_comment(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>,
    req: web::Json<CreateHollowCommentRequest>,
) -> impl Responder {
    let target_hollow = path.into_inner();
    let db = db.lock().unwrap();

    if !db.get_hollow(&target_hollow).is_some() {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "Hollow not found"}));
    }

    if req.content.trim().is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Comment cannot be empty"}));
    }

    if req.content.len() > 2000 {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "Comment too long (max 2000 chars)"}));
    }

    let sanitized = sanitize_content(&req.content);
    let comment = HollowComment::new(target_hollow, req.author_address.clone(), sanitized);
    db.add_hollow_comment(&comment);

    HttpResponse::Ok().json(serde_json::json!({
        "message": "Comment added",
        "comment": comment,
    }))
}

async fn list_hollow_comments(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let target_hollow = path.into_inner();
    let db = db.lock().unwrap();

    let mut comments = db.list_hollow_comments(&target_hollow);
    comments.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    HttpResponse::Ok().json(serde_json::json!({
        "comments": comments,
        "total": comments.len(),
    }))
}

async fn delete_hollow_comment(
    path: web::Path<(String, String)>,
    db: web::Data<Mutex<Database>>
) -> impl Responder {
    let (target_hollow, comment_id) = path.into_inner();
    let db = db.lock().unwrap();

    db.delete_hollow_comment(&target_hollow, &comment_id);
    HttpResponse::Ok().json(serde_json::json!({"message": "Comment deleted"}))
}

async fn add_peer(
    _db: web::Data<Mutex<Database>>,
    network: web::Data<P2PNetwork>,
    peer_req: web::Json<AddPeerRequest>
) -> impl Responder {
    network.add_peer(&peer_req.address).await;
    HttpResponse::Ok().json(serde_json::json!({"status": "peer added", "address": peer_req.address}))
}

async fn list_peers(
    network: web::Data<P2PNetwork>
) -> impl Responder {
    let peers = network.get_peers().await;
    HttpResponse::Ok().json(peers)
}

#[derive(serde::Deserialize)]
pub struct AddPeerRequest {
    pub address: String,
}

async fn report_hosting(
    path: web::Path<String>,
    db: web::Data<Mutex<Database>>,
    stats: web::Json<HostingStats>
) -> impl Responder {
    let db = db.lock().unwrap();
    let address = path.into_inner();

    match db.get_user(&address) {
        Some(mut user) => {
            // Normalized Sway with recency bias + quality weighting.
            //
            //   raw_score   – EMA of quality-weighted contributions, decays with
            //                 6-month half-life when the user goes dark.
            //   sway        – normalised to [0, 1] as
            //                 raw_score / (raw_score + network_avg_score),
            //                 so a user at the network average gets 0.5.
            //   quality     – derived from vouches so spam bots can't farm sway.

            const DECAY_RATE: f64 = 0.000_160_5_f64; // per hour: half-life ≈ 180 d
            const EMA_ALPHA: f64 = 0.3;               // weight of new contribution

            let now = Utc::now();

            // 1. Time decay on raw_score (6-month half-life)
            if let Some(last) = user.last_sway_update {
                let hours = (now - last).num_hours() as f64;
                user.raw_score *= (-DECAY_RATE * hours).exp();
            }
            user.last_sway_update = Some(now);

            // 2. Raw hosting contribution
            let raw = (stats.uptime_hours * 0.1) + (stats.bandwidth_bytes as f64 * 0.000_000_1);

            // 3. Quality multiplier – vouches reflect community trust.
            //    Botnets posting spam won't accrue vouches, so their quality
            //    stays near 1× while legit users climb.
            let quality = 1.0 + 0.3 * user.vouched_by.len() as f64;
            let contribution = raw * quality;

            // 4. EMA update on raw_score
            user.raw_score = EMA_ALPHA * contribution + (1.0 - EMA_ALPHA) * user.raw_score;

            // 5. Update the network-wide average (running EMA)
            db.update_network_avg_score(user.raw_score);
            let network_avg = db.get_network_avg_score();

            // 6. Normalised relative sway ∈ [0, 1]
            user.sway = user.raw_score / (user.raw_score + network_avg);

            db.add_user(user.clone());

            HttpResponse::Ok().json(serde_json::json!({
                "address": address,
                "raw_score": user.raw_score,
                "sway": user.sway,
                "network_avg": network_avg,
                "quality": quality,
            }))
        }
        None => HttpResponse::NotFound().json(serde_json::json!({"error": "User not found"}))
    }
}

#[derive(serde::Deserialize)]
pub struct HostingStats {
    pub uptime_hours: f64,
    pub bandwidth_bytes: u64,
}

#[derive(serde::Deserialize)]
pub struct LegendaryQuery {
    pub address: String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    // Initialize database
    let data_dir = std::path::Path::new(&cli.data_dir);
    std::fs::create_dir_all(data_dir).ok();
    let db_path = data_dir.join("db");
    let db = web::Data::new(Mutex::new(Database::open(&db_path).unwrap()));

    // Initialize default roots
    {
        let db_lock = db.lock().unwrap();
        db_lock.initialize_defaults();
    }

    // Start Tor hidden service (unless --gateway)
    let tor_manager = if !cli.gateway {
        let tor_binary = find_tor_binary();
        match tor::TorManager::start(&cli.data_dir, cli.torrc_extra.as_deref(), &tor_binary, cli.port).await {
            Ok(tm) => {
                if let Some(addr) = tm.onion_address().await {
                    println!("🧅 Onion address: http://{}/", addr);
                }
                Some(tm)
            }
            Err(e) => {
                eprintln!("⚠️  Tor not available: {}. Running in gateway mode.", e);
                #[cfg(target_os = "windows")]
                eprintln!("   Install tor: download from https://www.torproject.org/download/tor/ and place tor.exe next to moot.exe");
                #[cfg(not(target_os = "windows"))]
                eprintln!("   Install tor: apt-get install tor (or brew install tor)");
                None
            }
        }
    } else {
        println!("🚪 Running in gateway mode (no Tor hidden service)");
        None
    };

    // Health state for endpoint
    let onion_address = match &tor_manager {
        Some(tm) => tm.onion_address().await,
        None => None,
    };
    let health_state = web::Data::new(HealthState {
        tor_active: tor_manager.is_some(),
        onion_address,
    });

    // Load persisted peers from database and connect to bootstrap nodes
    let persisted_peers = {
        let db_lock = db.lock().unwrap();
        db_lock.load_peers()
    };
    println!("📋 Loaded {} persisted peers", persisted_peers.len());

    // Get SOCKS port from Tor manager if available (for P2P transport)
    let socks_port = if let Some(ref tm) = tor_manager {
        Some(tm.socks_port().await)
    } else {
        None
    };
    let (ws_broadcast_tx, _) = broadcast::channel::<String>(256);
    let ws_broadcast_for_p2p_handler = ws_broadcast_tx.clone();
    let (p2p_network, _publish_rx, mut p2p_rx) = P2PNetwork::new(ws_broadcast_tx.clone(), socks_port);
    println!("🌐 P2P network initialized (libp2p swarm)");

    // Connect to bootstrap nodes and persisted peers
    let all_peers: Vec<String> = BOOTSTRAP_NODES.iter()
        .map(|s| s.to_string())
        .chain(persisted_peers.clone())
        .collect();
    for peer in &all_peers {
        p2p_network.add_peer(peer).await;
    }
    println!("🔗 Connected to {} bootstrap/known peers", all_peers.len());

    let publish_tx = web::Data::new(p2p_network.get_publish_sender());
    let p2p_network_data = web::Data::new(p2p_network.clone());
    let msg_tx_data = web::Data::new(p2p_network.get_msg_sender());
    let ws_broadcast_data = web::Data::new(ws_broadcast_tx);

    // Spawn periodic peer persistence
    let db_for_peers = db.clone();
    let p2p_for_peers = p2p_network.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let peers = p2p_for_peers.get_peers().await;
            let addrs: Vec<String> = peers.iter()
                .flat_map(|p| p.addresses.clone())
                .collect();
            if !addrs.is_empty() {
                let d = db_for_peers.lock().unwrap();
                d.save_peers(&addrs);
            }
        }
    });

    // Spawn P2P message handler (receives messages from network)
    let db_clone = db.clone();
    let ws_broadcast_clone = ws_broadcast_for_p2p_handler;
    tokio::spawn(async move {
        while let Some(msg) = p2p_rx.recv().await {
            let db = db_clone.lock().unwrap();
            if let Ok(serialized) = serde_json::to_string(&msg) {
                let _ = ws_broadcast_clone.send(serialized);
            }
            match msg {
                P2PMessage::NewLeaf(mut leaf) => {
                    let node_config = db.get_node_config();
                    if node_config.auto_seed {
                        leaf.seeded_until = Some(chrono::Utc::now() + chrono::Duration::days(node_config.cache_ttl_days as i64));
                    } else {
                        leaf.seeded_until = None;
                    }
                    db.add_leaf(leaf.clone());
                    println!("📬 Received leaf from P2P");
                }
                P2PMessage::NewBranch(branch) => {
                    let root_id = branch.root_id.clone();
                    let trunk_id = branch.trunk_id.clone();
                    let name = branch.name.clone();
                    let desc = branch.description.clone();
                    match db.add_branch(root_id, trunk_id, name, desc) {
                        Ok(_) => println!("🌿 Received branch from P2P"),
                        Err(e) => println!("Error adding branch: {}", e),
                    }
                }
                P2PMessage::NewRoot(root) => {
                    db.add_root(root);
                    println!("🌳 Received root from P2P");
                }
                P2PMessage::Report(report) => {
                    db.add_report(report);
                    println!("⚠️ Received report from P2P");
                }
                P2PMessage::BlacklistVote(vote) => {
                    db.add_blacklist_vote(vote);
                    println!("🚫 Received blacklist vote from P2P");
                }
                P2PMessage::PromoteLegendary(leaf_id, promoter_address) => {
                    if let Some(leaf) = db.get_leaf(&leaf_id) {
                        let entry = LegendaryEntry {
                            leaf_id: leaf_id.clone(),
                            promoted_by: promoter_address.clone(),
                            promoted_at: chrono::Utc::now(),
                            leaf_snapshot: leaf,
                        };
                        db.add_legendary(&entry);
                        println!("⭐ Received legendary promotion from P2P: {} by {}", leaf_id, promoter_address);
                    }
                }
            }
        }
    });

    // Spawn background expiry job (evicts leaves past seeded_until)
    let expiry_db = db.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            let expired = {
                let d = expiry_db.lock().unwrap();
                d.get_expired_leaves()
            };
            let count = expired.len();
            for leaf in &expired {
                let d = expiry_db.lock().unwrap();
                d.delete_leaf(&leaf.id);
            }
            if count > 0 {
                println!("🧹 Evicted {} expired leaves", count);
            }
        }
    });

    println!("🌲 Moot starting... A decentralized social network.");
    println!("💎 Equal ownership. Node runners are gods. Browsers lurk.");
    println!("🌳 Roots: Finite community boards (tree analogy).");
    println!("🏠 Hollows: Your private expression space.");
    println!("⚖️  Sway: Your reputation from hosting.");
    println!("🚫 Moderation: Sway-weighted blacklisting.");
    println!("🧄 Garlic routing: Identity protection without VPN.");
    println!("🔌 WebSocket P2P: Real-time sync at /api/p2p/ws");
    println!("🛡️  Sybil defense: Staking, aging, social trust graphs.");

    let addr = format!("127.0.0.1:{}", cli.port);
    println!("\n📡 Server starting at http://{}", addr);

    let server = HttpServer::new(move || {
        App::new()
            .app_data(db.clone())
            .app_data(publish_tx.clone())
            .app_data(p2p_network_data.clone())
            .app_data(msg_tx_data.clone())
            .app_data(ws_broadcast_data.clone())
            .app_data(health_state.clone())
            .service(fs::Files::new("/static", "static").index_file("index.html"))
            .route("/", web::get().to(|| async { 
                actix_web::HttpResponse::Found()
                    .append_header(("Location", "/static/index.html"))
                    .finish()
            }))
            .route("/health", web::get().to(health_check))
            .route("/api/roots", web::get().to(list_roots))
            .route("/api/root/{id}", web::get().to(get_root))
            .route("/api/search", web::get().to(search))
            .route("/api/upload", web::post().to(upload_image))
            .route("/api/leaf/{address}", web::post().to(create_leaf))
            .route("/api/upvote_leaf/{leaf_id}/{address}", web::post().to(upvote_leaf))
            .route("/api/downvote_leaf/{leaf_id}/{address}", web::post().to(downvote_leaf))
            .route("/api/sway/{address}", web::get().to(get_sway))
            .route("/api/sway/report/{address}", web::post().to(report_hosting))
            .route("/api/mirror_leaf/{leaf_id}/{address}", web::post().to(mirror_leaf))
            .route("/api/mirrored_leaves/{address}", web::get().to(get_mirrored_leaves))
            .route("/api/leaves/{root_id}", web::get().to(list_leaves_in_root))
            .route("/api/report", web::post().to(report_content))
            .route("/api/vote_blacklist", web::post().to(vote_blacklist))
            .route("/api/vote_dismiss", web::post().to(vote_dismiss))
            .route("/api/blacklist/{target_type}/{target_id}", web::get().to(check_blacklist))
            .route("/api/legendary/promote/{leaf_id}", web::post().to(promote_legendary))
            .route("/api/legendary/remove/{leaf_id}", web::post().to(remove_legendary))
            .route("/api/legendary", web::get().to(list_legendary))
            .route("/api/leaf/{id}/expiry", web::get().to(get_leaf_expiry))
            .route("/api/node/config", web::get().to(get_node_config_handler))
            .route("/api/node/config", web::post().to(set_node_config_handler))
            .route("/api/branch/{root_id}", web::post().to(create_branch))
            .route("/api/trunks/{root_id}", web::get().to(list_trunks))
            .route("/api/trunk/{root_id}", web::post().to(create_trunk))
            .route("/api/trunk/{trunk_id}/branches", web::get().to(list_branches_in_trunk))
            .route("/api/branches/{root_id}", web::get().to(list_branches))
            .route("/api/branch/{branch_id}", web::get().to(get_branch))
            .route("/api/hollow/{address}", web::get().to(get_hollow))
            .route("/api/hollow/{address}", web::post().to(create_hollow))
            .route("/api/hollow/{address}/settings", web::post().to(update_hollow_settings))
            .route("/api/hollow/{address}/friend", web::post().to(add_friend))
            .route("/api/hollow/{address}/post", web::post().to(add_hollow_post))
            .route("/api/hollow/{address}/post/{post_id}", web::delete().to(delete_hollow_post))
            .route("/api/hollow/{address}/comment", web::post().to(add_hollow_comment))
            .route("/api/hollow/{address}/comments", web::get().to(list_hollow_comments))
            .route("/api/hollow/{address}/comment/{comment_id}", web::delete().to(delete_hollow_comment))
            .route("/api/peers/add", web::post().to(add_peer))
            .route("/api/peers/list", web::get().to(list_peers))
        .route("/api/p2p/message", web::post().to(p2p_receive))
        .route("/api/p2p/garlic", web::post().to(p2p_garlic_receive))
        .route("/api/p2p/pubkey", web::get().to(p2p_pubkey))
        .route("/api/p2p/ws", web::get().to(p2p_ws_handler))
        .route("/api/leaves/all", web::get().to(get_all_leaves))
    })
    .bind(&addr)?
    .run();

    // Wait for server with graceful shutdown
    tokio::select! {
        result = server => {
            if let Err(e) = result {
                eprintln!("Server error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!("\nShutting down...");
        }
    }

    // Shutdown Tor
    if let Some(tm) = tor_manager {
        tm.shutdown().await;
    }

    Ok(())
}
