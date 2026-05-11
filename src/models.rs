use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Leaf {
    pub id: String,
    pub author_address: String,
    pub author_name: Option<String>,
    pub content: String,
    pub media_urls: Vec<String>,
    pub root: String,
    pub branch: String,
    pub parent_leaf_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub upvotes: i32,
    pub downvotes: i32,
    pub mirrors: Vec<String>,
    pub is_mirrored: bool,
    pub is_deleted: bool,
    #[serde(default)]
    pub seeded_until: Option<DateTime<Utc>>,
}

impl Leaf {
    #[allow(dead_code)]
    pub fn new(author_address: String, content: String, root: String, branch: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            author_address,
            author_name: None,
            content,
            media_urls: vec![],
            root,
            branch,
            parent_leaf_id: None,
            created_at: Utc::now(),
            upvotes: 0,
            downvotes: 0,
            mirrors: vec![],
            is_mirrored: false,
            is_deleted: false,
            seeded_until: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vote {
    pub voter_address: String,
    pub target_id: String,
    pub target_type: VoteTarget,
    pub vote_type: VoteType,
    pub sway_weight: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteTarget {
    Leaf,
    User,
    Root,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteType {
    Upvote,
    Downvote,
    Mirror,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    pub id: String,
    pub name: String,
    pub designator: String,
    pub created_at: DateTime<Utc>,
    pub branches: Vec<String>,
}

impl Root {
    pub fn new(id: String, name: String, designator: String) -> Self {
        Self {
            id,
            name,
            designator,
            created_at: Utc::now(),
            branches: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trunk {
    pub id: String,
    pub root_id: String,
    pub name: String,
    pub description: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

impl Trunk {
    pub fn new(root_id: String, name: String, description: String, created_by: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            root_id,
            name,
            description,
            created_by,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    pub id: String,
    pub root_id: String,
    pub trunk_id: Option<String>,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub parent_branch_id: Option<String>,
}

impl Branch {
    pub fn new(root_id: String, trunk_id: Option<String>, name: String, description: String) -> Self {
        let datetime_str = Utc::now().to_rfc3339();
        let input = format!("{}:{}:{}", root_id, name, datetime_str);
        let mut hasher = seahash::SeaHasher::default();
        std::hash::Hash::hash(&input, &mut hasher);
        let id = format!("{:x}", std::hash::Hasher::finish(&hasher));
        
        Self {
            id,
            root_id,
            trunk_id,
            name,
            description,
            created_at: Utc::now(),
            parent_branch_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub address: String,
    /// Exponential moving average of contribution quality (0 to ∞).
    /// Decays with a 6-month half-life when inactive.
    #[serde(default)]
    pub raw_score: f64,
    /// Normalized sway relative to the network average.  Computed as
    /// `raw_score / (raw_score + network_avg_score)` → 0 – 1.
    #[serde(default)]
    pub sway: f64,
    pub total_leaves: i32,
    pub mirrored_leaves: Vec<String>,
    pub vouched_by: Vec<String>,
    pub vouch_requirement: f64,
    pub is_banned: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub last_sway_update: Option<DateTime<Utc>>,
}

impl User {
    #[allow(dead_code)]
    pub fn new(address: String) -> Self {
        Self {
            address,
            raw_score: 0.0,
            sway: 0.0,
            total_leaves: 0,
            mirrored_leaves: vec![],
            vouched_by: vec![],
            vouch_requirement: 1.0,
            is_banned: false,
            created_at: Utc::now(),
            last_sway_update: None,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sway {
    pub address: String,
    pub hosting_bytes: u64,
    pub uptime_hours: f64,
    pub total_sway: f64,
    pub last_updated: DateTime<Utc>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mirror {
    pub leaf_id: String,
    pub mirror_address: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportCategory {
    Spam,
    Misinformation,
    NsfwUnmarked,
    Harassment,
    HateSpeech,
    IllegalContent,
}

impl ReportCategory {
    pub fn severity(&self) -> u8 {
        match self {
            Self::Spam => 1,
            Self::Misinformation => 2,
            Self::NsfwUnmarked => 2,
            Self::Harassment => 3,
            Self::HateSpeech => 5,
            Self::IllegalContent => 5,
        }
    }
    pub fn label(&self) -> &str {
        match self {
            Self::Spam => "Spam",
            Self::Misinformation => "Misinformation",
            Self::NsfwUnmarked => "NSFW (unmarked)",
            Self::Harassment => "Harassment",
            Self::HateSpeech => "Hate Speech",
            Self::IllegalContent => "Illegal Content",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportStatus {
    Open,
    Blacklisted,
    Dismissed,
}

impl Default for ReportStatus {
    fn default() -> Self { Self::Open }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub id: String,
    pub reporter_address: String,
    pub target_type: ReportTarget,
    pub target_id: String,
    pub category: ReportCategory,
    pub severity: u8,
    #[serde(default)]
    pub staked_sway: f64,
    #[serde(default)]
    pub status: ReportStatus,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportTarget {
    Leaf,
    User,
    Root,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistVote {
    pub report_id: String,
    pub voter_address: String,
    pub sway_weight: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DismissVote {
    pub report_id: String,
    pub voter_address: String,
    pub sway_weight: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistEntry {
    pub target_id: String,
    pub target_type: ReportTarget,
    pub reason: String,
    pub total_sway: f64,
    pub blacklisted_at: DateTime<Utc>,
}

// Hollow - private user space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hollow {
    pub owner_address: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub created_at: DateTime<Utc>,
    pub private_posts: Vec<HollowPost>,
    pub settings: HollowSettings,
    pub friends: Vec<String>,
    pub music_url: Option<String>,
    pub video_embed: Option<String>,
    pub social_links: Vec<SocialLink>,
    pub custom_html: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialLink {
    pub platform: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HollowSettings {
    pub is_public: bool,
    pub allow_mirrors: bool,
    pub theme: String,
    pub custom_css: Option<String>,
    pub font_size: Option<String>,
    pub text_color: Option<String>,
    pub bg_color: Option<String>,
    pub animation: Option<String>,
}

impl Hollow {
    pub fn new(owner_address: String) -> Self {
        Self {
            owner_address,
            display_name: None,
            bio: None,
            created_at: Utc::now(),
            private_posts: vec![],
            settings: HollowSettings {
                is_public: true, // Default to public for MySpace vibe
                allow_mirrors: true,
                theme: "default".to_string(),
                custom_css: None,
                font_size: None,
                text_color: None,
                bg_color: None,
                animation: None,
            },
            friends: vec![],
            music_url: None,
            video_embed: None,
            social_links: vec![],
            custom_html: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HollowPost {
    pub id: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub is_public: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLeafRequest {
    pub author_name: Option<String>,
    pub content: String,
    pub media_urls: Option<Vec<String>>,
    pub root: String,
    pub branch: String,
    pub parent_leaf_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBranchRequest {
    pub name: String,
    pub description: String,
    pub trunk_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTrunkRequest {
    pub name: String,
    pub description: String,
    pub created_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHollowPostRequest {
    pub content: String,
    pub is_public: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRequest {
    pub reporter_address: String,
    pub target_type: ReportTarget,
    pub target_id: String,
    pub category: ReportCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistVoteRequest {
    pub voter_address: String,
    pub report_id: String,
    pub vote_type: VoteType,
}

// ── Content Lifecycle ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub cache_ttl_days: u64,
    pub cache_max_mb: u64,
    pub legendary_cache_max_mb: u64,
    pub auto_seed: bool,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            cache_ttl_days: 7,
            cache_max_mb: 1024,
            legendary_cache_max_mb: 512,
            auto_seed: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegendaryEntry {
    pub leaf_id: String,
    pub promoted_by: String,
    pub promoted_at: DateTime<Utc>,
    pub leaf_snapshot: Leaf,
}

// ── Hollow Comments ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HollowComment {
    pub id: String,
    pub target_hollow: String,
    pub author_address: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

impl HollowComment {
    pub fn new(target_hollow: String, author_address: String, content: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            target_hollow,
            author_address,
            content,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateHollowCommentRequest {
    pub author_address: String,
    pub content: String,
}
