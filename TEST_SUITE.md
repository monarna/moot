# 🧪 Moot Test Suite - Manual Testing Guide

**App URL**: http://127.0.0.1:8080/static/index.html  
**API Base**: http://127.0.0.1:8080/api

---

## 🏁 Pre-Test Setup

1. **Start the server** (if not running):
   ```bash
   cd /mnt/Data/Projects/Moot && cargo run
   # Or if already built:
   ./target/debug/moot
   ```

2. **Check server is running**:
   ```bash
   curl http://127.0.0.1:8080/health
   # Expected: {"project":"moot","status":"ok"}
   ```

---

## 📋 Test Checklist

### 🌳 Phase 1: Roots (Boards)

**Test 1.1: List all roots**
```bash
curl http://127.0.0.1:8080/api/roots | jq '.[].id'
# Expected: "a", "g", "v", "mu", "lit", "tv", "sci", "pol"
# (8 default roots)
```

**Test 1.2: Get specific root**
```bash
curl http://127.0.0.1:8080/api/root/g | jq '.name'
# Expected: "/g/ - Technology"
```

**Test 1.3: Root not found**
```bash
curl http://127.0.0.1:8080/api/root/nonexistent
# Expected: {"error": "Root not found"}
```

---

### 🌿 Phase 2: Branches (Sub-boards)

**Test 2.1: Create branch in root**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"id":"hardware","name":"Hardware","description":"Hardware discussion"}' \
  http://127.0.0.1:8080/api/branch/g
# Expected: Returns branch JSON with id="hardware"
```

**Test 2.2: List branches in root (empty then populated)**
```bash
# First check empty
curl http://127.0.0.1:8080/api/branches/g
# Expected: [] (empty array)

# After Test 2.1, check again
curl http://127.0.0.1:8080/api/branches/g | jq '.[].name'
# Expected: "Hardware"
```

**Test 2.3: Get specific branch**
```bash
curl http://127.0.0.1:8080/api/branch/hardware | jq '.description'
# Expected: "Hardware discussion"
```

**Test 2.4: Duplicate branch (should fail)**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"id":"hardware","name":"Hardware2","description":"Duplicate"}' \
  http://127.0.0.1:8080/api/branch/g
# Expected: {"error": "Branch ID already exists"}
```

---

### 🍃 Phase 3: Leaves (Posts)

**Test 3.1: Create a leaf (post)**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"author_name":"test_user","content":"Hello from Moot!","root":"g","branch":"hardware"}' \
  http://127.0.0.1:8080/api/leaf/0xTestUser
# Expected: Returns leaf JSON with id, content, etc.
```

**Test 3.2: Upvote a leaf**
```bash
# First get the leaf ID from Test 3.1 output
LEAF_ID="<paste_leaf_id_here>"

curl -X POST http://127.0.0.1:8080/api/upvote_leaf/$LEAF_ID/0xVoter
# Expected: Returns leaf with upvotes: 1
```

**Test 3.3: Mirror a leaf**
```bash
curl -X POST http://127.0.0.1:8080/api/mirror_leaf/$LEAF_ID/0xMirrorer
# Expected: Returns leaf with mirrors array containing "0xMirrorer"
```

**Test 3.4: Get mirrored leaves**
```bash
curl http://127.0.0.1:8080/api/mirrored_leaves/0xMirrorer
# Expected: Returns array with the mirrored leaf
```

**Test 3.5: List leaves in root**
```bash
curl http://127.0.0.1:8080/api/leaves/g
# Expected: Returns array of leaves in /g/ root
```

---

### 🏠 Phase 4: Hollows (Private Spaces)

**Test 4.1: Create hollow**
```bash
curl -X POST http://127.0.0.1:8080/api/hollow/0xHollowUser
# Expected: Returns hollow JSON with default settings
```

**Test 4.2: Get hollow (public=false, limited info)**
```bash
curl http://127.0.0.1:8080/api/hollow/0xHollowUser
# Expected: Shows only owner_address, display_name, bio (no private_posts)
```

**Test 4.3: Update hollow settings**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"is_public":true,"allow_mirrors":true,"theme":"dark"}' \
  http://127.0.0.1:8080/api/hollow/0xHollowUser/settings
# Expected: {"message": "Settings updated"}
```

**Test 4.4: Add private post to hollow**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"content":"My secret thought","is_public":false}' \
  http://127.0.0.1:8080/api/hollow/0xHollowUser/post
# Expected: Returns post JSON with id
```

**Test 4.5: Get hollow (now public=true, full info)**
```bash
curl http://127.0.0.1:8080/api/hollow/0xHollowUser | jq '.private_posts | length'
# Expected: 1 (the post from Test 4.4)
```

**Test 4.6: Delete hollow post**
```bash
POST_ID="<paste_post_id_from_4.4>"
curl -X DELETE http://127.0.0.1:8080/api/hollow/0xHollowUser/post/$POST_ID
# Expected: {"message": "Post deleted"}
```

---

### ⚖️ Phase 5: Sway (Reputation)

**Test 5.1: Get user sway (new user)**
```bash
curl http://127.0.0.1:8080/api/sway/0xNewUser
# Expected: {"address": "0xNewUser", "sway": 0.0, "message": "User not found..."}
```

**Test 5.2: Get user sway (after posting)**
```bash
# After creating a leaf in Phase 3, check sway
curl http://127.0.0.1:8080/api/sway/0xTestUser
# Expected: sway > 0 (user created when posting)
```

---

### 🚫 Phase 6: Moderation (Sway-weighted Blacklisting)

**Test 6.1: Report content**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"reporter_address":"0xModerator","target_type":"Leaf","target_id":"'$LEAF_ID'","reason":"Spam"}' \
  http://127.0.0.1:8080/api/report
# Expected: Returns report_id
```

**Test 6.2: Vote on blacklist**
```bash
REPORT_ID="<paste_report_id_from_6.1>"
curl -X POST -H "Content-Type: application/json" \
  -d '{"voter_address":"0xModerator","report_id":"'$REPORT_ID'","vote_type":"Upvote"}' \
  http://127.0.0.1:8080/api/vote_blacklist
# Expected: {"message": "Vote recorded", "total_sway": <value>, "threshold": 100.0}
```

**Test 6.3: Check blacklist status**
```bash
curl http://127.0.0.1:8080/api/blacklist/leaf/$LEAF_ID
# Expected: {"target_id": "...", "is_blacklisted": false} (until threshold met)
```

---

### 🌐 Phase 7: P2P Network (HTTP-Based Sync)

**Test 7.1: Check P2P started**
```bash
# Look at server logs
tail -20 /tmp/moot.log
# Expected: "🌐 P2P network initialized (HTTP-based)"
```

**Test 7.2: Test P2P message endpoint**
```bash
# Test receiving P2P messages
curl -X POST -H "Content-Type: application/json" \
  -d '{"NewLeaf":{"id":"test123","author_address":"0x123...","author_name":"test","content":"P2P test","root":"g","branch":"hardware","parent_leaf_id":null,"created_at":"2026-05-07T00:00:00Z","upvotes":0,"downvotes":0,"mirrors":[],"is_mirrored":false,"is_deleted":false}}' \
  http://127.0.0.1:8080/api/p2p/message
# Expected: {"status": "ok"}
```

**Test 7.3: Test get all leaves for sync**
```bash
curl http://127.0.0.1:8080/api/leaves/all
# Expected: Array of all leaves in JSON
```

**Test 7.4: Test P2P sync between two nodes (manual)**
```bash
# Terminal 1: Start node A on port 8080
cd /mnt/Data/Projects/Moot && ./target/debug/moot

# Terminal 2: Start node B on port 8081 (need to modify code or use docker)
# Then add node B as peer to node A
# Create leaf on node A, verify it syncs to node B
```

---

### 📱 Phase 8: Web UI (PWA)

**Test 8.1: Open in browser**
```
Open: http://127.0.0.1:8080/static/index.html
Expected: Tree-themed UI with "MOOT" header
```

**Test 8.2: Test UI functionality**
1. Click "[Roots]" nav - Should show 8 roots
2. Click a root (e.g., "/g/ - Technology") - Should show branches
3. Try to create a leaf - Use "0xTestUser" as address
4. Check "My Hollow" - Should create/prompt for hollow
5. Check "Sway" - Should show your reputation

**Test 8.3: PWA installation (mobile)**
1. Open in mobile browser (or Chrome DevTools device mode)
2. Look for "Add to Home Screen" or install prompt
3. Install - Should work offline (service worker registered)

---

## 🐛 Error Handling Tests

**Test E.1: Invalid endpoint**
```bash
curl http://127.0.0.1:8080/api/nonexistent
# Expected: 404 or Actix error page
```

**Test E.2: Invalid JSON**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d 'invalid json' \
  http://127.0.0.1:8080/api/leaf/0xTest
# Expected: 400 Bad Request
```

**Test E.3: Missing fields**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"content":"only content"}' \
  http://127.0.0.1:8080/api/leaf/0xTest
# Expected: 400 or default values used
```

---

## 📊 Performance & Load Tests (Optional)

**Test P.1: Multiple rapid requests**
```bash
for i in {1..10}; do
  curl -s http://127.0.0.1:8080/api/roots &
done
wait
# Expected: All requests succeed
```

**Test P.2: Create 100 leaves**
```bash
for i in {1..100}; do
  curl -X POST -H "Content-Type: application/json" \
    -d "{\"author_name\":\"user$i\",\"content\":\"Post $i\",\"root\":\"g\",\"branch\":\"hardware\"}" \
    http://127.0.0.1:8080/api/leaf/0xUser$i
done
# Expected: All created successfully
```

---

## ✅ Test Completion Checklist

- [ ] Phase 1: Roots (3/3 tests)
- [ ] Phase 2: Branches (4/4 tests)
- [ ] Phase 3: Leaves (5/5 tests)
- [ ] Phase 4: Hollows (6/6 tests)
- [ ] Phase 5: Sway (2/2 tests)
- [ ] Phase 6: Moderation (3/3 tests)
- [ ] Phase 7: P2P (2/2 tests)
- [ ] Phase 8: Web UI (3/3 tests)
- [ ] Error Handling (3/3 tests)

**Total: 31 tests**

---

## 🔒 Security Tests (New)

**Test S.1: Input sanitization (XSS prevention)**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"author_name":"test","content":"<script>alert(\"XSS\")</script>Hello","root":"g","branch":"hardware"}' \
  http://127.0.0.1:8080/api/leaf/0xTestUser
# Expected: Content should have <script> tags removed, only "Hello" remains
```

**Test S.2: Address validation**
```bash
# Invalid address (no 0x prefix)
curl -X POST -H "Content-Type: application/json" \
  -d '{"author_name":"test","content":"test","root":"g","branch":"hardware"}' \
  http://127.0.0.1:8080/api/leaf/invalid_address
# Expected: {"error": "Invalid address format"}

# Valid address
curl -X POST -H "Content-Type: application/json" \
  -d '{"author_name":"test","content":"test","root":"g","branch":"hardware"}' \
  http://127.0.0.1:8080/api/leaf/0x1234567890abcdef1234567890abcdef12345678
# Expected: Should accept (if other fields valid)
```

**Test S.3: Signature format validation**
```bash
# Test with invalid signature format (if signature field added to API)
# This test is for future when signature verification is integrated
```

---

## 🧪 Phase 9: Content Lifecycle UI Tests

**Test 9.1: Check leaf expiry info**
```bash
# First get a leaf ID from the leaves list
LEAF_ID=$(curl -s http://127.0.0.1:8080/api/leaves/all | python3 -c "import json,sys; d=json.load(sys.stdin); print(d[0]['id'] if d else '')")
curl http://127.0.0.1:8080/api/leaf/$LEAF_ID/expiry
# Expected: JSON with seeded_until, expires_in_seconds, is_expired fields
```

**Test 9.2: Promote leaf to legendary**
```bash
curl -X POST "http://127.0.0.1:8080/api/legendary/promote/$LEAF_ID?address=0xTestUser"
# Expected: {"message":"Promoted to legendary","leaf_id":"..."}
```

**Test 9.3: List legendary entries**
```bash
curl "http://127.0.0.1:8080/api/legendary?address=0xTestUser" | jq '. | length'
# Expected: 1 (from Test 9.2)
```

**Test 9.4: Remove legendary entry**
```bash
curl -X POST "http://127.0.0.1:8080/api/legendary/remove/$LEAF_ID?address=0xTestUser"
# Expected: {"message":"Removed from legendary"}
curl "http://127.0.0.1:8080/api/legendary?address=0xTestUser" | jq '. | length'
# Expected: 0
```

**Test 9.5: Get node config**
```bash
curl http://127.0.0.1:8080/api/node/config | jq '.'
# Expected: {"cache_ttl_days":7,"cache_max_mb":1024,"legendary_cache_max_mb":512,"auto_seed":true}
```

**Test 9.6: Update node config**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"cache_ttl_days":14,"cache_max_mb":2048,"legendary_cache_max_mb":1024,"auto_seed":false}' \
  http://127.0.0.1:8080/api/node/config
# Expected: {"message":"Node config updated"}

# Verify
curl http://127.0.0.1:8080/api/node/config | jq '.cache_ttl_days'
# Expected: 14

# Reset to defaults for other tests
curl -X POST -H "Content-Type: application/json" \
  -d '{"cache_ttl_days":7,"cache_max_mb":1024,"legendary_cache_max_mb":512,"auto_seed":true}' \
  http://127.0.0.1:8080/api/node/config
```

---

## 🧪 Phase 10: WebSocket P2P Tests

**Test 10.1: Connect to WebSocket endpoint**
```bash
# Use websocat or wscat to test WS connection
# Install: cargo install websocat
echo "{}" | timeout 3 websocat ws://127.0.0.1:8080/api/p2p/ws 2>&1 || echo "Connection attempted"
# Expected: Connection succeeds (will close after timeout since we sent empty JSON)
```

**Test 10.2: WebSocket receives broadcast on leaf creation**
```bash
# Terminal 1: Listen for WS messages
# websocat ws://127.0.0.1:8080/api/p2p/ws

# Terminal 2: Create a leaf
# curl -X POST -H "Content-Type: application/json" \
#   -d '{"author_name":"ws_test","content":"WS test","root":"g","branch":"hardware"}' \
#   http://127.0.0.1:8080/api/leaf/0xWSTest

# Expected: Terminal 1 receives P2PMessage::NewLeaf JSON
```

---

## 🧪 Phase 11: Hollow Customization Tests

**Test 11.1: Update hollow with customizations**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{
    "display_name":"Test User",
    "bio":"Hello from Moot!",
    "is_public":true,
    "theme":"dark",
    "custom_css":"body { color: red; }",
    "custom_html":"<p>Hello World</p>",
    "music_url":"https://example.com/song.mp3",
    "video_embed":"<iframe src=\"https://example.com/video\"></iframe>",
    "font_size":"16px",
    "text_color":"#ff0000",
    "bg_color":"#000000",
    "animation":"fadeIn"
  }' \
  http://127.0.0.1:8080/api/hollow/0xTestUser/settings
# Expected: {"message":"Hollow updated"}
```

**Test 11.2: Verify custom fields in hollow response**
```bash
curl http://127.0.0.1:8080/api/hollow/0xTestUser | jq '.display_name'
# Expected: "Test User"
curl http://127.0.0.1:8080/api/hollow/0xTestUser | jq '.music_url'
# Expected: "https://example.com/song.mp3"
```

**Test 11.3: Add friend to hollow**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"friend_address":"0xFriend123"}' \
  http://127.0.0.1:8080/api/hollow/0xTestUser/friend
# Expected: {"message":"Friend added"}

# Verify
curl http://127.0.0.1:8080/api/hollow/0xTestUser | jq '.friends'
# Expected: ["0xFriend123"]
```

**Test 11.4: Add public post to hollow**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"content":"Public post!","is_public":true}' \
  http://127.0.0.1:8080/api/hollow/0xTestUser/post
# Expected: Returns post JSON
```

**Test 11.5: Reset hollow settings**
```bash
curl -X POST -H "Content-Type: application/json" \
  -d '{"display_name":"","bio":"","is_public":true,"theme":"default","custom_css":"","custom_html":"","music_url":"","video_embed":"","font_size":"","text_color":"","bg_color":"","animation":""}' \
  http://127.0.0.1:8080/api/hollow/0xTestUser/settings
# Expected: {"message":"Hollow updated"}
```

---

## ✅ Test Completion Checklist

- [ ] Phase 1: Roots (3/3 tests)
- [ ] Phase 2: Branches (4/4 tests)
- [ ] Phase 3: Leaves (5/5 tests)
- [ ] Phase 4: Hollows (6/6 tests)
- [ ] Phase 5: Sway (2/2 tests)
- [ ] Phase 6: Moderation (3/3 tests)
- [ ] Phase 7: P2P (2/2 tests)
- [ ] Phase 8: Web UI (3/3 tests)
- [ ] Phase 9: Content Lifecycle (6/6 tests)
- [ ] Phase 10: WebSocket P2P (2/2 tests)
- [ ] Phase 11: Hollow Customizations (5/5 tests)
- [ ] Error Handling (3/3 tests)
- [ ] Security (3/3 tests)

**Total: 47 tests**

---

## 🚨 Known Issues / Limitations

1. **Images not synced across peers** - Image URLs are relative to the uploading node. Cross-node image fetch not yet implemented.
2. **P2P is HTTP-based + WebSocket** - Works but full libp2p not integrated
3. **Sway not auto-updating** - Only updates when hosting report submitted
4. **Crypto signature not enforced** - Address validation works but Ed25519 signature verification is optional
5. **Garlic routing is one-hop** - Multi-hop forwarding not yet implemented
6. **No authentication** - Addresses are strings, no key custody

---

## 🎉 After Testing

If all tests pass:
- ✅ Core functionality works
- ✅ P2P sync (HTTP + WebSocket)
- ✅ Content lifecycle (TTL, legendary, config)
- ✅ Hollow customizations
- ✅ Ready for deployment

**Report bugs or issues at**: [Your issue tracker here]

---

**Happy Testing! 🧪🔬**
