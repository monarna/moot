use sled::Db;
use crate::models::*;
use chrono::Utc;

pub struct Database {
    db: Db,
}

impl Database {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, sled::Error> {
        let db = sled::open(path)?;
        Ok(Self { db })
    }
    
    // Leaf operations
    pub fn add_leaf(&self, leaf: Leaf) {
        let key = format!("leaf:{}", leaf.id);
        let value = serde_json::to_vec(&leaf).unwrap();
        self.db.insert(key.as_bytes(), value).unwrap();
    }
    
    pub fn get_leaf(&self, id: &str) -> Option<Leaf> {
        let key = format!("leaf:{}", id);
        self.db.get(key.as_bytes()).ok().flatten().and_then(|v| {
            serde_json::from_slice(&v).ok()
        })
    }
    
    pub fn list_leaves(&self) -> Vec<Leaf> {
        self.db.scan_prefix(b"leaf:")
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .collect()
    }
    
    // Vote operations
    pub fn add_vote(&self, vote: Vote) {
        let key = format!("vote:{}:{}", vote.target_id, vote.voter_address);
        let value = serde_json::to_vec(&vote).unwrap();
        self.db.insert(key.as_bytes(), value).unwrap();
    }
    
    // Root operations
    pub fn add_root(&self, root: Root) {
        let key = format!("root:{}", root.id);
        let value = serde_json::to_vec(&root).unwrap();
        self.db.insert(key.as_bytes(), value).unwrap();
    }
    
    pub fn get_root(&self, id: &str) -> Option<Root> {
        let key = format!("root:{}", id);
        self.db.get(key.as_bytes()).ok().flatten().and_then(|v| {
            serde_json::from_slice(&v).ok()
        })
    }
    
    pub fn list_roots(&self) -> Vec<Root> {
        self.db.scan_prefix(b"root:")
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .collect()
    }
    
    // User operations
    pub fn add_user(&self, user: User) {
        let key = format!("user:{}", user.address);
        let value = serde_json::to_vec(&user).unwrap();
        self.db.insert(key.as_bytes(), value).unwrap();
    }
    
    pub fn get_user(&self, address: &str) -> Option<User> {
        let key = format!("user:{}", address);
        self.db.get(key.as_bytes()).ok().flatten().and_then(|v| {
            serde_json::from_slice(&v).ok()
        })
    }
    
    // Report operations for moderation
    pub fn add_report(&self, report: Report) {
        let key = format!("report:{}", report.id);
        let value = serde_json::to_vec(&report).unwrap();
        self.db.insert(key.as_bytes(), value).unwrap();
        
        // Add to reports by target index
        let target_key = format!("reports_by_target:{}", report.target_id);
        let mut reports: Vec<String> = self.db.get(target_key.as_bytes())
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default();
        reports.push(report.id.clone());
        self.db.insert(target_key.as_bytes(), serde_json::to_vec(&reports).unwrap()).unwrap();
    }
    
    pub fn get_report(&self, id: &str) -> Option<Report> {
        let key = format!("report:{}", id);
        self.db.get(key.as_bytes()).ok().flatten().and_then(|v| {
            serde_json::from_slice(&v).ok()
        })
    }
    
    #[allow(dead_code)]
    pub fn get_reports_for_target(&self, target_id: &str) -> Vec<Report> {
        let target_key = format!("reports_by_target:{}", target_id);
        let report_ids: Vec<String> = self.db.get(target_key.as_bytes())
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default();
        
        report_ids.iter()
            .filter_map(|id| self.get_report(id))
            .collect()
    }
    
    // Blacklist vote operations
    pub fn add_blacklist_vote(&self, vote: BlacklistVote) {
        let key = format!("blacklist_vote:{}:{}", vote.report_id, vote.voter_address);
        let value = serde_json::to_vec(&vote).unwrap();
        self.db.insert(key.as_bytes(), value).unwrap();
    }

    pub fn get_blacklist_votes(&self, report_id: &str) -> Vec<BlacklistVote> {
        self.db.scan_prefix(format!("blacklist_vote:{}", report_id).as_bytes())
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .collect()
    }

    pub fn get_total_sway_for_report(&self, report_id: &str) -> f64 {
        self.get_blacklist_votes(report_id)
            .iter()
            .map(|v| v.sway_weight)
            .sum()
    }

    // Dismiss vote operations
    pub fn add_dismiss_vote(&self, vote: DismissVote) {
        let key = format!("dismiss_vote:{}:{}", vote.report_id, vote.voter_address);
        let value = serde_json::to_vec(&vote).unwrap();
        self.db.insert(key.as_bytes(), value).unwrap();
    }

    pub fn get_dismiss_votes(&self, report_id: &str) -> Vec<DismissVote> {
        self.db.scan_prefix(format!("dismiss_vote:{}", report_id).as_bytes())
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .collect()
    }

    pub fn get_total_dismiss_sway_for_report(&self, report_id: &str) -> f64 {
        self.get_dismiss_votes(report_id)
            .iter()
            .map(|v| v.sway_weight)
            .sum()
    }

    // Resolve a report: check if blacklist or dismiss thresholds are met,
    // update status, and handle sway payouts.
    // Returns the final status if resolved, None if still open.
    pub fn resolve_report(&self, report_id: &str) -> Option<ReportStatus> {
        let mut report = self.get_report(report_id)?;
        if !matches!(report.status, ReportStatus::Open) {
            return Some(report.status); // already resolved
        }

        let total_blacklist = self.get_total_sway_for_report(report_id);
        let total_dismiss = self.get_total_dismiss_sway_for_report(report_id);
        let threshold = report.severity as f64 * 20.0;

        if total_blacklist >= threshold {
            // Blacklisted — reporter gets stake back + bonus
            if report.staked_sway > 0.0 {
                if let Some(mut user) = self.get_user(&report.reporter_address) {
                    user.raw_score += (report.staked_sway + 0.01) * 5.0; // small raw_score bump
                    // re-compute sway
                    let avg = self.get_network_avg_score();
                    user.sway = user.raw_score / (user.raw_score + avg);
                    self.add_user(user);
                }
            }
            report.status = ReportStatus::Blacklisted;
            self.add_report(report);
            Some(ReportStatus::Blacklisted)
        } else if total_dismiss >= threshold {
            // Dismissed — reporter loses their stake entirely
            report.status = ReportStatus::Dismissed;
            self.add_report(report);
            Some(ReportStatus::Dismissed)
        } else {
            None // still open
        }
    }
    
    // Blacklist entry operations
    pub fn add_blacklist_entry(&self, entry: BlacklistEntry) {
        let key = format!("blacklist:{}:{}", entry.target_type.clone() as u8, entry.target_id);
        let value = serde_json::to_vec(&entry).unwrap();
        self.db.insert(key.as_bytes(), value).unwrap();
    }
    
    pub fn is_blacklisted(&self, target_id: &str, target_type: ReportTarget) -> bool {
        let key = format!("blacklist:{}:{}", target_type as u8, target_id);
        self.db.get(key.as_bytes()).ok().flatten().is_some()
    }
    
    // Trunk operations
    pub fn add_trunk(&self, root_id: String, name: String, description: String, created_by: String) -> Result<String, &'static str> {
        let existing = self.list_trunks_in_root(&root_id);
        if existing.len() >= 20 {
            return Err("Maximum trunks per root (20) reached");
        }
        if existing.iter().any(|t| t.name == name) {
            return Err("Trunk with this name already exists in this root");
        }
        let trunk = Trunk::new(root_id, name, description, created_by);
        let id = trunk.id.clone();
        let key = format!("trunk:{}", id);
        let value = serde_json::to_vec(&trunk).unwrap();
        self.db.insert(key.as_bytes(), value).unwrap();
        Ok(id)
    }

    pub fn get_trunk(&self, id: &str) -> Option<Trunk> {
        let key = format!("trunk:{}", id);
        self.db.get(key.as_bytes()).ok().flatten().and_then(|v| serde_json::from_slice(&v).ok())
    }

    pub fn list_trunks_in_root(&self, root_id: &str) -> Vec<Trunk> {
        self.db.scan_prefix(b"trunk:")
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .filter(|t: &Trunk| t.root_id == root_id)
            .collect()
    }

    #[allow(dead_code)]
    pub fn delete_trunk(&self, id: &str) {
        let key = format!("trunk:{}", id);
        self.db.remove(key.as_bytes()).ok();
    }

    // Branch operations
    pub fn add_branch(&self, root_id: String, trunk_id: Option<String>, name: String, description: String) -> Result<String, &'static str> {
        let existing_branches = self.list_branches_in_root(&root_id);
        if existing_branches.iter().any(|b| b.name == name) {
            return Err("Branch with this name already exists in this root");
        }
        
        let branch = Branch::new(root_id.clone(), trunk_id, name.clone(), description.clone());
        let id = branch.id.clone();
        
        let key = format!("branch:{}", id);
        let value = serde_json::to_vec(&branch).unwrap();
        self.db.insert(key.as_bytes(), value).unwrap();
        
        if let Some(mut root) = self.get_root(&root_id) {
            if !root.branches.contains(&id) {
                root.branches.push(id.clone());
                self.add_root(root);
            }
        }
        
        Ok(id)
    }
    
    pub fn get_branch(&self, id: &str) -> Option<Branch> {
        let key = format!("branch:{}", id);
        self.db.get(key.as_bytes()).ok().flatten().and_then(|v| {
            serde_json::from_slice(&v).ok()
        })
    }
    
    pub fn list_branches_in_root(&self, root_id: &str) -> Vec<Branch> {
        self.db.scan_prefix(b"branch:")
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .filter(|b: &Branch| b.root_id == root_id)
            .collect()
    }

    pub fn list_branches_in_trunk(&self, trunk_id: &str) -> Vec<Branch> {
        self.db.scan_prefix(b"branch:")
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .filter(|b: &Branch| b.trunk_id.as_deref() == Some(trunk_id))
            .collect()
    }

    pub fn leaf_count_in_branch(&self, branch_id: &str) -> usize {
        self.list_leaves().iter().filter(|l| l.branch == branch_id && !l.is_deleted).count()
    }
    
    #[allow(dead_code)]
    pub fn list_all_branches(&self) -> Vec<Branch> {
        self.db.scan_prefix(b"branch:")
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .collect()
    }
    
    // Hollow operations
    pub fn add_hollow(&self, hollow: Hollow) {
        let key = format!("hollow:{}", hollow.owner_address);
        let value = serde_json::to_vec(&hollow).unwrap();
        self.db.insert(key.as_bytes(), value).unwrap();
    }
    
    pub fn get_hollow(&self, owner_address: &str) -> Option<Hollow> {
        let key = format!("hollow:{}", owner_address);
        self.db.get(key.as_bytes()).ok().flatten().and_then(|v| {
            serde_json::from_slice(&v).ok()
        })
    }
    
    pub fn update_hollow(&self, hollow: Hollow) -> Result<(), &'static str> {
        self.add_hollow(hollow);
        Ok(())
    }
    
    #[allow(dead_code)]
    pub fn update_hollow_settings(&self, owner_address: &str, settings: HollowSettings) -> Result<(), &'static str> {
        match self.get_hollow(owner_address) {
            Some(mut hollow) => {
                hollow.settings = settings;
                self.add_hollow(hollow);
                Ok(())
            }
            None => Err("Hollow not found"),
        }
    }
    
    pub fn add_hollow_post(&self, owner_address: &str, post: HollowPost) -> Result<(), &'static str> {
        match self.get_hollow(owner_address) {
            Some(mut hollow) => {
                hollow.private_posts.push(post);
                self.add_hollow(hollow);
                Ok(())
            }
            None => Err("Hollow not found"),
        }
    }
    
    pub fn delete_hollow_post(&self, owner_address: &str, post_id: &str) -> Result<(), &'static str> {
        match self.get_hollow(owner_address) {
            Some(mut hollow) => {
                hollow.private_posts.retain(|p| p.id != post_id);
                self.add_hollow(hollow);
                Ok(())
            }
            None => Err("Hollow not found"),
        }
    }
    
    // Search across leaves, branches, and roots
    pub fn search(&self, query: &str) -> serde_json::Value {
        let q = query.to_lowercase();
        let leaves: Vec<Leaf> = self.list_leaves().into_iter()
            .filter(|l| !l.is_deleted && (
                l.content.to_lowercase().contains(&q) ||
                l.author_name.as_deref().unwrap_or("").to_lowercase().contains(&q)
            ))
            .collect();

        let branches: Vec<Branch> = self.db.scan_prefix(b"branch:")
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .filter(|b: &Branch| {
                b.name.to_lowercase().contains(&q) ||
                b.description.to_lowercase().contains(&q)
            })
            .collect();

        let roots: Vec<Root> = self.db.scan_prefix(b"root:")
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .filter(|r: &Root| {
                r.name.to_lowercase().contains(&q) ||
                r.designator.to_lowercase().contains(&q)
            })
            .collect();

        serde_json::json!({
            "leaves": leaves,
            "branches": branches,
            "roots": roots,
            "query": query,
            "total": leaves.len() + branches.len() + roots.len(),
        })
    }

    // Initialize default roots (26 letters of the alphabet) with a General trunk each
    pub fn initialize_defaults(&self) {
        let default_roots = vec![
            ("a", "/a\\", "Anime & Manga"),
            ("b", "/b\\", "Books & Literature"),
            ("c", "/c\\", "Cooking & Food"),
            ("d", "/d\\", "Drugs (18+)"),
            ("e", "/e\\", "Economy & Finance"),
            ("f", "/f\\", "Fashion & Style"),
            ("g", "/g\\", "Gaming (Video & Tabletop)"),
            ("h", "/h\\", "Hobbies"),
            ("i", "/i\\", "Internet Culture & Memes"),
            ("j", "/j\\", "Jobs & Careers"),
            ("k", "/k\\", "Knowledge & Science"),
            ("l", "/l\\", "Local & Community"),
            ("m", "/m\\", "Music & Audio"),
            ("n", "/n\\", "News & Current Events"),
            ("o", "/o\\", "Outdoors & Nature"),
            ("p", "/p\\", "Porn (18+)"),
            ("q", "/q\\", "Q&A & Advice"),
            ("r", "/r\\", "Roleplay & RPG"),
            ("s", "/s\\", "Sports & Fitness"),
            ("t", "/t\\", "Technology (Gadgets, IT, Software, Automotive)"),
            ("u", "/u\\", "University & Student Life"),
            ("v", "/v\\", "TV & Movies"),
            ("w", "/w\\", "World & Travel"),
            ("x", "/x\\", "Paranormal & Unexplained"),
            ("y", "/y\\", "Social & Relationships"),
            ("z", "/z\\", "Random & Misc"),
        ];
        
        for (id, designator, name) in default_roots {
            let root = Root::new(id.to_string(), name.to_string(), designator.to_string());
            self.add_root(root);
            // Every root gets a General trunk for uncategorized branches
            let existing_trunks = self.list_trunks_in_root(id);
            if existing_trunks.is_empty() {
                let _ = self.add_trunk(
                    id.to_string(),
                    "General".to_string(),
                    "General discussion — everything that doesn't fit elsewhere".to_string(),
                    "system".to_string(),
                );
            }
        }
    }

    // ── Network-average score (for relative sway normalisation) ──

    /// Return the network-wide average raw_score.
    /// Initialises to 1.0 if not set (so new users start at sway ≈ 0).
    pub fn get_network_avg_score(&self) -> f64 {
        self.db
            .get(b"meta:network_avg_score")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or(1.0)
    }

    /// Update the running network average via EMA.
    /// Called after every sway update so the relative baseline drifts smoothly.
    pub fn update_network_avg_score(&self, user_raw_score: f64) {
        const ALPHA: f64 = 0.001; // smooth follower – 0.1 % weight per update
        let current = self.get_network_avg_score();
        let updated = ALPHA * user_raw_score + (1.0 - ALPHA) * current;
        self.db
            .insert(b"meta:network_avg_score", serde_json::to_vec(&updated).unwrap())
            .unwrap();
    }

    /// Compute live sway from raw_score relative to the network average.
    /// Returns 0.0 when raw_score is 0 or the user is missing.
    pub fn compute_sway(&self, user: &Option<User>) -> f64 {
        match user {
            Some(u) if u.raw_score > 0.0 => {
                let avg = self.get_network_avg_score();
                u.raw_score / (u.raw_score + avg)
            }
            _ => 0.0,
        }
    }

    // ── Content Lifecycle ──

    /// Return leaves whose `seeded_until` is in the past (excluding legendary).
    pub fn get_expired_leaves(&self) -> Vec<Leaf> {
        let now = Utc::now();
        self.db.scan_prefix(b"leaf:")
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice::<Leaf>(&v).ok())
            .filter(|l| {
                // Only expired if seeded_until is set AND in the past
                if let Some(until) = l.seeded_until {
                    until < now && !l.is_deleted
                } else {
                    false
                }
            })
            .collect()
    }

    /// Delete a leaf from the local database entirely.
    pub fn delete_leaf(&self, leaf_id: &str) {
        let key = format!("leaf:{}", leaf_id);
        let _ = self.db.remove(key.as_bytes());
    }

    // ── Node Config ──

    pub fn get_node_config(&self) -> NodeConfig {
        self.db
            .get(b"meta:node_config")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default()
    }

    pub fn set_node_config(&self, config: &NodeConfig) {
        self.db
            .insert(b"meta:node_config", serde_json::to_vec(config).unwrap())
            .unwrap();
    }

    // ── Legendary ──

    pub fn add_legendary(&self, entry: &LegendaryEntry) {
        let key = format!("legendary:{}:{}", entry.promoted_by, entry.leaf_id);
        self.db
            .insert(key.as_bytes(), serde_json::to_vec(entry).unwrap())
            .unwrap();
    }

    pub fn remove_legendary(&self, address: &str, leaf_id: &str) {
        let key = format!("legendary:{address}:{leaf_id}");
        let _ = self.db.remove(key.as_bytes());
    }

    pub fn list_legendary(&self, address: &str) -> Vec<LegendaryEntry> {
        self.db.scan_prefix(format!("legendary:{address}").as_bytes())
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .collect()
    }

    pub fn is_legendary(&self, address: &str, leaf_id: &str) -> bool {
        let key = format!("legendary:{address}:{leaf_id}");
        self.db.get(key.as_bytes()).ok().flatten().is_some()
    }

    // ── Hollow Comments ──

    pub fn add_hollow_comment(&self, comment: &HollowComment) {
        let key = format!("hollow_comment:{}:{}", comment.target_hollow, comment.id);
        self.db
            .insert(key.as_bytes(), serde_json::to_vec(comment).unwrap())
            .unwrap();
    }

    pub fn list_hollow_comments(&self, target_hollow: &str) -> Vec<HollowComment> {
        self.db.scan_prefix(format!("hollow_comment:{target_hollow}").as_bytes())
            .filter_map(|r| r.ok())
            .filter_map(|(_, v)| serde_json::from_slice(&v).ok())
            .collect()
    }

    pub fn delete_hollow_comment(&self, target_hollow: &str, comment_id: &str) {
        let key = format!("hollow_comment:{target_hollow}:{comment_id}");
        let _ = self.db.remove(key.as_bytes());
    }

    // ── Peer Persistence ──

    pub fn save_peers(&self, peers: &[String]) {
        let value = serde_json::to_vec(peers).unwrap();
        self.db.insert(b"meta:peers", value).unwrap();
    }

    pub fn load_peers(&self) -> Vec<String> {
        self.db
            .get(b"meta:peers")
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default()
    }
}
