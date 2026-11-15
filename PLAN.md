# Mongo Studio — Development Plan (`PLAN.md`)

เอกสารแผนการทำงานสำหรับพัฒนา **Mongo Studio** (MongoDB Desktop Admin Tool ด้วยภาษา Rust + `egui` / `eframe` / `egui_dock`) โดยถอดโครงสร้างฟีเจอร์จาก [README.md](./README.md) และจัดลำดับการทำงานตามข้อกำหนดใน [AGENTS.md](./AGENTS.md) และ `.agents/`

---

## สถานะภาพรวมของโปรเจกต์

| Phase | หัวข้อ | สถานะ | Git Commit / หมายเหตุ |
| :--- | :--- | :--- | :--- |
| **Phase 1** | UI Framework & Workspace Shell | ✅ เสร็จสิ้น | `c1266f1` |
| **Phase 2** | Live MongoDB Connection & Security | ✅ เสร็จสิ้น | `5e282ef` |
| **Phase 3** | Database Explorer (Lazy-Loading) & Admin RBAC | ✅ เสร็จสิ้น | `fdc60e3`, `0d9f910`, `905ec2d` (Polish) |
| **Phase 4** | Live Document Manager & Full CRUD / Query | ✅ เสร็จสิ้น | `596ac1c` (41/41 tests ผ่าน) |
| **Phase 5** | Advanced Features, Export/Import & Release | ✅ เสร็จสิ้น | `2bcda74` (70/70 tests ผ่าน) |
| **Feature+** | SSH Tunnel Connection (`ip:port`, `user/pass`, `id_ecdsa`) | ✅ เสร็จสิ้น | รองรับ `ssh ip:port` (`user/pass`, `id_ecdsa` + Keychain, 88/88 tests ผ่าน) |
| **Feature+** | Clean State on Release Build (Exclude Mock DB) | ✅ เสร็จสิ้น | เอา mock DB และ mock tabs ออกจาก Release build เริ่มต้นแบบ clean state, 90/90 tests ผ่าน |
| **Feature+** | User Auth DB Identity & Multi-DB Disambiguation | ✅ เสร็จสิ้น | แยก user ตาม `(username, auth_db)` แก้ปัญหา collision 9 ตัว, 98/98 tests ผ่าน |
| **Feature+** | Workspace & Project Organization (Left Panel) | ✅ เสร็จสิ้น | Customer 🏢 -> Folder 📁 -> Connection (Tag) ตามภาพอ้างอิง, 115/115 tests ผ่าน |
| **Feature+** | Workspace Management Polish & Connection Env Colors | ✅ เสร็จสิ้น | Manage modal ใช้ได้จริง, Dropdown ใน Add/Edit Modal, Context menu คลิกขวา, Env text colors, 123/123 tests ผ่าน |

---

## กติกาการทำงานและ Team Ownership

ยึดตาม [AGENTS.md](./AGENTS.md) และ `.agents/agents/`:

1. **Git Workflow**: ต้อง `git commit` ก่อนขึ้น Phase ใหม่ทุกครั้ง (และแยก commit ตาม feature ย่อยได้)
2. **Sequential Agent Execution**: ห้ามให้หลาย Agent แก้ไฟล์พร้อมกันเด็ดขาด
3. **Code Standards**: เปิดและปฏิบัติตาม [`.agents/skills/rust-skills/SKILL.md`](./.agents/skills/rust-skills/SKILL.md) ก่อนเริ่มทุก Phase
4. **Module Ownership**:
   - `core-dev`: `src/core/`, `src/database/`, `src/models/`, `src/query/`
   - `ui-dev`: `src/ui/`, `src/theme/`, `src/workspace/`, `src/app.rs` (ห้าม block UI thread ด้วย Synchronous I/O และเลี่ยงการ allocate ใน hot path)
   - `qa-tester`: `tests/` (ตรวจสอบ `cargo clippy --all-targets` และ `cargo test` รวมถึงทดสอบกับเซิร์ฟเวอร์จริงเมื่อมี)

---

## Phase 1 — UI Framework & Workspace Shell ✅

- [x] Desktop Application Shell ด้วย `eframe` / `egui` และ Title / Menu Bar (`File`, `Edit`, `View`, `Help`)
- [x] VS Code-inspired Theme System: Dark Theme (ค่าเริ่มต้น) และ Light Theme (`src/theme/`)
- [x] Activity Bar (Connections, Saved Queries, Aggregation, Schema Analyzer, Settings) พร้อม `egui-phosphor` Font Icons และ Logo SVG (`assets/logo.svg`)
- [x] Collapsible Sidebar & Connection Tree View (`src/ui/sidebar.rs`, `src/ui/components/tree_view.rs`)
- [x] Multi-Tab Docking Workspace ด้วย `egui_dock` (`src/ui/tab_bar.rs`, `src/workspace/tabs.rs`)
- [x] Bottom Panel 4 แท็บ: `TERMINAL`, `OUTPUT`, `QUERY HISTORY`, `PROBLEMS` (`src/workspace/panels.rs`)
- [x] Status Bar & Command Palette (`Ctrl+P` / `Cmd+P`) พร้อม Keyboard Shortcuts (`Ctrl+B`, `Ctrl+J`)
- [x] Workspace Layout Persistence (`src/workspace/persistence.rs`)

---

## Phase 2 — Live MongoDB Connection & Security ✅

- [x] เชื่อมต่อ MongoDB จริงด้วย `mongodb` + `bson` และ Multi-threaded `tokio` Runtime
- [x] `ConnectionManager` & `ConnectionClient` (`ping_uri`, `connect_and_introspect`, `normalize_uri`, `redact_uri`)
- [x] Modal เพิ่ม/ทดสอบการเชื่อมต่อ (`Add Connection`, `Test Connection`) พร้อมบันทึก/โหลดโปรไฟล์ใน `connections.json` (`directories`)
- [x] จัดเก็บรหัสผ่านอย่างปลอดภัยด้วย OS Keychain (`keyring`) และซ่อนรหัสผ่านใน Log/Tooltip
- [x] ระบบ Authentication แบบปลอดภัย: ไม่เขียนรหัสผ่านแบบ Plaintext ลงดิสก์, ล้างรหัสผ่านชั่วคราวเมื่อ `Disconnect` (หากไม่ได้ติ๊ก Save in Keychain) และเด้งถามรหัสผ่านใหม่อัตโนมัติเมื่อกด Expand
- [x] รองรับ Connection Profiles ใน Sidebar: Auto-Connect เมื่อกด Expand, `Edit Connection...`, `Disconnect`, `Remove Profile`, จำกัดชื่อโปรไฟล์สูงสุด 30 ตัวอักษร

---

## Phase 3 — Database Explorer & Admin User Management ✅ *(กำลังเก็บงาน Polish)*

### สิ่งที่ทำเสร็จแล้ว (`fdc60e3`, `0d9f910`)
- [x] **Fast Concurrent Lazy-Loading**: ค้นหา Databases & Collections แบบขนาน (`JoinSet` + `Semaphore(16)`) สร้างเป็น Lightweight Stub (`loaded: false`) ทำให้เชื่อมต่อ 95+ ฐานข้อมูลเสร็จภายใน ~112 ms
- [x] **On-Demand Collection Introspection**: โหลดเอกสาร (ตัวอย่าง 50 รายการ), Indexes, Document Count และวิเคราะห์ Schema เฉพาะเมื่อคลิกเปิดหรือกด Refresh Collection
- [x] **Add Database**: เมนูคลิกขวา `Add Database...` บน Profile พร้อม Popup สร้างฐานข้อมูลและ Collection เริ่มต้น
- [x] **UI/Theme Fixes**: Scrollbar แบบไม่ลอยทับเนื้อหาและกลมกลืนกับพื้นหลัง, สีข้อความใน Light Mode ชัดเจน, ปุ่ม `Add Connection` ด้านบน Sidebar
- [x] **Admin User & Role Management (RBAC)** (`src/models/user.rs`, `src/database/user_service.rs`):
  - [x] แสดงโหนด `Users (N)` ใน Sidebar เมื่อเชื่อมต่อด้วยสิทธิ์ Admin (`usersInfo`)
  - [x] หน้า User Permissions Inspector แสดงตาราง Database Roles แบบ Dynamic Table เต็มพื้นที่พร้อม Scroll
  - [x] แถวแรกเป็นตัวเพิ่มสิทธิ์ (`Database` พร้อมช่องค้นหา + `Role` + `+ Add`), แถวเดิมแก้ไข Role inline ได้ และมีปุ่ม `Cancel` / `Apply` มุมล่างขวา
  - [x] ปุ่ม `Change Password...` (Popup พร้อม Confirm Password & Show Passwords)
  - [x] เมนูคลิกขวา & Popup `Create User...`, `Duplicate User...` (คัดลอก Role ทั้งหมด) และ `Delete User...` (มี Modal ยืนยัน)
  - [x] ป้องกันบัญชีระบบ `admin` อย่างเคร่งครัดทั้งใน UI และ Backend (`ensure_not_protected_admin`)

### งาน Polish ก่อนขึ้น Phase 4 *(เสร็จสิ้นตาม `.agents/` และ `rust-skills`)*
- [x] (`core-dev`) เพิ่ม Async Keychain / Async `save_profiles_async` (`spawn_blocking`), Single-Client User Ops (`*_and_list`), และ `AppEvent` สำหรับรายงาน Error จริงไม่กลืน Error
- [x] (`ui-dev`) เชื่อมต่อ Async Keychain/Persistence และ Error Events ใน `src/app.rs` + ลดการ Allocate รายเฟรมใน `src/ui/tab_bar.rs` และ `src/workspace/tabs.rs`
- [x] (`qa-tester`) ตรวจสอบ `.unwrap()` ใน `src/` (0 จุด) + เพิ่ม Tests สำหรับกรณี Command Failure, Async Keychain และ `Duplicate User` + รัน `cargo clippy --all-targets` และ `cargo test` ผ่าน 24/24 (รวมทดสอบจริงกับ `192.168.100.243:27017`)
- [x] (`git`) Commit งาน Polish และ `PLAN.md` ให้เรียบร้อยก่อนเริ่ม Phase 4

---

## Phase 4 — Document Manager & Live Query / CRUD ✅

เป้าหมาย: เปลี่ยนหน้า Collection จากโหมดอ่านตัวอย่าง (50 docs / mock filter) ให้เป็นเครื่องมือจัดการเอกสารและ Query บน MongoDB จริงแบบครบวงจร

### 4.1 Database & Collection Explorer Operations (ส่วนที่เหลือจาก `README.md`)
- [x] `core-dev`: เพิ่ม `create_collection`, `drop_collection`, และ `drop_database` ใน `DatabaseService` / `CollectionService` พร้อมป้องกัน system DB (`admin`, `local`, `config`) และ `system.*`
- [x] `ui-dev`: เพิ่ม Context Menu คลิกขวาที่ Database (`Create Collection...`, `Drop Database...`) และที่ Collection (`Open Collection`, `Refresh Collection`, `Drop Collection...`) พร้อม Confirmation Modal ป้องกันการลบพลาด

### 4.2 Live Document Query, Sort, Projection & Pagination
- [x] `core-dev`: เพิ่ม `DocumentService::query_documents` (async MongoDB `find` รองรับ `filter`, `projection`, `sort`, `skip`, `limit` `1..=500`, `count_documents` แบบ concurrent) และจัดการ Timeout / Parse Extended JSON Error อย่างชัดเจน
- [x] `ui-dev`: เชื่อม Filter Bar, Sort Bar, Projection Input, Limit Selector (`20`/`50`/`100`/`250`), Find / Reset, และตัวควบคุม **Pagination** (`< Prev` / Page X of Y / แสดงช่วง / `Next >`) ใน `src/ui/tab_bar.rs`

### 4.3 Real Document CRUD (Insert / Edit / Delete)
- [x] `core-dev`: เพิ่ม `insert_document`, `update_document` (แก้เอกสารทั้งก้อนด้วย `replace_one` พร้อมป้องกันการแก้ `_id`), และ `delete_document` (`delete_one` ตาม `_id` จริง) ใน `DocumentService`
- [x] `ui-dev`:
  - แทนที่ `InsertMockDocument` ใน flow หลักด้วย **Insert / Edit Document JSON Editor Modal** (ตรวจสอบ Extended JSON syntax & `_id` immutability แบบ real-time ก่อนส่ง)
  - เพิ่มปุ่ม `Edit` / `Delete` บนแต่ละแถวใน Table View และ JSON Tree View พร้อม Confirmation Dialog ก่อนลบเอกสาร

### 4.4 Dynamic Table Columns & Enhanced JSON Tree View
- [x] `core-dev`: ปรับโมเดล `DocumentRow` ให้รองรับการ cache `cached_json: Option<Value>`, `extract_columns`, `cell_display_value`, `extract_bson_id` และดึงคอลัมน์จริงจาก BSON โดยไม่ต้อง re-parse ทุกเฟรม
- [x] `ui-dev`: ปรับ `data_grid.rs` ให้แสดงคอลัมน์แบบ Dynamic จาก Collection พร้อม Horizontal/Vertical Scroll และปรับ `json_viewer.rs` ให้กาง Nested Object / Array แบบ Recursive พร้อมสีรองรับทั้ง Dark/Light Mode และปุ่ม `Copy JSON` / `Copy _id` / `Edit` / `Delete`

### 4.5 Verification (`qa-tester`)
- [x] ตรวจสอบ `.unwrap()` ใน production `src/` (0 จุด) และเขียน Integration Tests สำหรับ Offline Guards + Live Query (`filter`/`projection`/`sort`/`limit`/`skip`), Create/Drop Collection & Database และ Insert/Update/Delete Document บน `192.168.100.243:27017`
- [x] ตรวจสอบ `cargo clippy --all-targets` (0 warnings) และ `cargo test` ผ่าน 41/41 (8 lib + 10 `core_workspace_tests` + 15 `database_tests` + 8 `query_tests`)

---

## Phase 5 — Advanced Features, Export/Import & Release ✅

เป้าหมาย: เพิ่มเครื่องมือระดับสูงสำหรับ DBA/Developer และการแพ็กเกจแอปพลิเคชัน

### 5.1 Aggregation Pipeline Builder
- [x] `core-dev`: เพิ่ม Aggregation Runner ใน `src/query/aggregation.rs` & `DocumentService::run_aggregation` รองรับ Stage Builder และ Raw JSON Pipeline
- [x] `ui-dev`: สร้าง UI สำหรับ Add / Remove / Reorder Stages (`$match`, `$group`, `$project`, `$sort`, `$limit`, `$unwind`, `$lookup`, `$addFields`, `$count`), โหมด JSON Editor และแสดงผลลัพธ์จาก MongoDB จริง

### 5.2 Index Management
- [x] `core-dev`: เพิ่ม `create_index` (รองรับ Key Spec, Unique, Sparse, Background/Name) และ `drop_index` ใน `src/database/index_service.rs` (ห้ามลบ `_id_` และป้องกัน `system.*`)
- [x] `ui-dev`: เพิ่มปุ่ม `Create Index...` และ `Drop Index` (พร้อม Confirmation) ในแท็บ `Indexes`

### 5.3 Enhanced Schema Explorer & Syntax Highlighting
- [x] `core-dev`: เพิ่มตัวเลือก Sample Size (`50..1000`) สำหรับคำนวณ Schema ใน `SchemaAnalyzer` และรองรับ Nested Field Paths (เช่น `address.city`, `items[]`, `items[].sku`) พร้อม `type_breakdown`
- [x] `ui-dev`: เชื่อม Activity Bar `Schema Analyzer`, แสดง Type Distribution Bar และนำ `syntect` มาทำ Syntax Highlighting ใน JSON / Query / Aggregation / Index Editor

### 5.4 Persistent Query History, Saved Queries & Terminal
- [x] `core-dev`: บันทึก Query History (`MAX_QUERY_HISTORY = 100`) และ Saved Queries ลงไฟล์คอนฟิก `queries.json` (`directories`) แบบ atomic (`spawn_blocking`) + `ShellService` ใน Bottom Panel
- [x] `ui-dev`: เชื่อมหน้า `Saved Queries` และ `QUERY HISTORY` ให้คลิกโหลดหรือรันซ้ำได้บน Connection ที่เลือกอยู่จริง รวมถึงพัฒนา `TERMINAL` Shell (`help`, `show dbs`, `use`, `show collections`, `find`, `countDocuments`, `runCommand`)

### 5.5 Export / Import Data
- [x] `core-dev`: สร้าง `TransferService` (JSON, JSON Lines `.jsonl`, RFC 4180 CSV) แบบ Streaming / Batched Async บน Tokio (`spawn_blocking`)
- [x] `ui-dev`: ใช้ `rfd` (Native File Dialog) สำหรับ Export ผลลัพธ์ Query หรือ Import เข้า Collection

### 5.6 Quality, Cross-Platform & Release Polish
- [x] ตรวจสอบ Code Quality ด้วย `cargo fmt --check`, `cargo clippy --all-targets` (0 warnings), `cargo test` ผ่าน 70/70 (21 lib + 14 `core_workspace_tests` + 24 `database_tests` + 11 `query_tests` รวมทดสอบจริงกับ `192.168.100.243:27017`)
- [x] ตั้งค่า `[profile.release]` (`opt-level = 3`, `lto = true`, `codegen-units = 1`, `strip = true`) *(หมายเหตุ: `cargo build --release` ไม่สามารถเขียนเสร็จบนเครื่องนี้ชั่วคราวเนื่องจากพื้นที่ดิสก์เหลือ ~195 MiB)*

---

## Feature+ — SSH Tunnel Connection (`ip:port`, `user/pass`, `id_ecdsa`) ✅

- [x] `core-dev`: เพิ่ม `SshTunnelConfig`, `SshAuthMethod` (`Password` / `PrivateKey` เริ่มต้น `~/.ssh/id_ecdsa`), `SshTunnelRegistry` (`src/database/ssh_tunnel.rs`) จัดการ `ssh -N -T` port forwarding แบบ async บน Tokio พร้อม OS Keychain (`{conn_id}:ssh`), `AskpassTempDir` (`0o700`) และ URI rewriting (`directConnection=true`)
- [x] `ui-dev`: เพิ่มส่วนตั้งค่า SSH Tunnel (`Connect via SSH Tunnel (ip:port)`, `SSH Host (ip:port)`, `SSH Username`, Auth: `Password` / `SSH Key (id_ecdsa)` + Browse key / Passphrase, Save in Keychain) ใน Modal เพิ่ม/แก้ไข Connection, `[SSH]` badge ใน sidebar tree และปิด Tunnel อัตโนมัติเมื่อ Disconnect/Remove/Drop
- [x] `qa-tester`: ตรวจสอบความปลอดภัย (0 `.unwrap()` ใน `src/`, ล้าง secrets เมื่อสลับ Auth mode/ปิด modal, `redact_uri` IPv4/Query `@` fix) และเพิ่ม Tests ผ่านครบ 88/88 (31 lib + 16 `core_workspace_tests` + 30 `database_tests` + 11 `query_tests`)

---

## Feature+ — Workspace & Project Organization (Left Panel) 🔄

เป้าหมาย: ปรับโครงสร้างเมนู Connection ที่ Left Panel ให้รองรับการจัดกลุ่มตาม Workspace (Customer / Client), Project Folder และ Environment พร้อม Quick Access (Favorites / Recent) และ Empty State ตามแบบร่าง [design/WORKSPACE_DESIGN.md](./design/WORKSPACE_DESIGN.md)

### Step 1: Data Model & Storage Backward Compatibility (`core-dev`) ✅
- [x] สร้างโมเดล `src/models/workspace.rs`:
  - `EnvironmentTag` (Auto-detection จากชื่อ เช่น `-dev`, `-test`, `-staging`, `-uat`, `-prod` และฟังก์ชันสี/สไตล์)
  - `WorkspaceItem` และ `ProjectFolder` สำหรับจัดกลุ่มโครงสร้าง Customer -> Folder -> Connection
  - Helper functions สำหรับจัดกลุ่ม `Vec<ConnectionConfig>` เป็นโครงสร้าง Tree และฟิลเตอร์ตาม Search / Workspace
- [x] ปรับปรุง `ConnectionConfig` ใน `src/models/connection.rs`:
  - เพิ่มฟิลด์ `workspace: Option<String>`
  - เพิ่มฟิลด์ `folder: Option<String>`
  - เพิ่มฟิลด์ `is_favorite: bool`
  - เพิ่มฟิลด์ `environment: Option<EnvironmentTag>`
  - ใช้ `#[serde(default)]` ทุกฟิลด์เพื่อ backward compatibility 100% กับ `connections.json` เดิม
- [x] Export module ใน `src/models/mod.rs`

### Step 2: Events & App State Management (`core-dev`) ✅
- [x] เพิ่ม Events ใน `src/core/events.rs`:
  - `ToggleFavoriteConnection { connection_id: Uuid }`
  - `FilterWorkspaceView { workspace_name: Option<String> }`
  - `CreateWorkspace { name: String }`
  - `CreateProjectFolder { workspace: String, folder_name: String }`
  - `MoveConnectionToFolder { connection_id: Uuid, workspace: Option<String>, folder: Option<String> }`
  - `DeleteWorkspace { name: String }`
  - `DeleteProjectFolder { workspace: String, folder_name: String }`
  - `OpenManageWorkspacesModal`
  - `CloseManageWorkspacesModal`
- [x] ขยาย `AppState` ใน `src/core/state.rs`:
  - ฟิลด์ `active_workspace_filter: Option<String>`
  - ฟิลด์ `recent_connections: Vec<Uuid>` (บันทึก ID ล่าสุด 10 รายการเมื่อ Connect สำเร็จ)
  - ฟิลด์ `show_manage_workspaces_modal: bool`
  - Helper methods: `toggle_favorite`, `push_recent_connection`, `set_connection_workspace_folder`, `delete_workspace`, `delete_project_folder`, `favorite_count`, `recent_count`
- [x] เชื่อมต่อ App Event loop ใน `src/app.rs` พร้อมบันทึก `connections.json` อัตโนมัติเมื่อมีการเปลี่ยนแปลง

### Step 3: UI Left Panel & 3-Tier Tree View (`ui-dev`) ✅
- [x] ปรับปรุง `src/ui/sidebar.rs`:
  - ช่อง Search Full Width พร้อม Hint `⌘ K` (กรองทั้งชื่อและ env)
  - แถบ Quick Access: `⭐ Favorites` (พร้อม Badge จำนวน) และ `🕒 Recent`
  - หัวข้อ `WORKSPACES` พร้อม Dropdown สลับดู All Workspaces หรือเฉพาะ Workspace
  - ปุ่ม `+` รวม (Smart Add Popover) เลือกว่าจะเพิ่ม Connection, Folder หรือ Workspace
  - Empty State: แสดง Icon Folder + "No workspaces yet" + ปุ่ม "Manage Workspaces" เมื่อยังไม่มีการจัดกลุ่ม
- [x] ปรับปรุง `src/ui/components/tree_view.rs`:
  - โครงสร้าง Tree 3 ระดับ: `Customer 🏢` $\rightarrow$ `Project 📁` $\rightarrow$ `Connection`
  - แสดง Environment Tag กะทัดรัด (`DEV`, `TEST`, `PROD`) นำหน้าชื่อ Connection
  - รองรับการคลิกเลือก Connection (Highlight แถวแบบโค้งมน) และขยายดู Databases / Collections ได้เหมือนเดิม
  - ปุ่ม `+` ปรากฏขึ้นบนแถวเมื่อ Hover เพื่อเพิ่ม Folder หรือ Connection ภายใต้ Node นั้นได้โดยตรง

### Step 4: Manage Workspaces Modal & Import/Export (`ui-dev`) ✅
- [x] ปุ่ม `⚙️ Manage Workspaces` ที่ Footer ด้านล่างสุดของ Left Panel
- [x] Modal จัดการ Workspaces: สร้าง/เปลี่ยนชื่อ/ลบ Workspace และย้าย Connection
- [x] รองรับการ Export / Import โครงสร้าง Workspace เป็น JSON

### Step 5: QA Testing & Polish (`qa-tester`) ✅
- [x] เขียน Unit Tests ใน `tests/core_workspace_tests.rs`:
  - ทดสอบ Serde Backward Compatibility ของ `ConnectionConfig`
  - ทดสอบการตรวจจับ `EnvironmentTag::detect_from_name`
  - ทดสอบการจัดกลุ่มโครงสร้าง Tree และการค้นหากรอง
- [x] ตรวจสอบ Contrast และขนาด Component ทั้ง Light Mode & Dark Mode
- [x] รัน `cargo test` (115/115 tests ผ่าน), `cargo clippy --all-targets -- -D warnings` (0 warnings) และ `cargo fmt --check`

### Step 6: Full Workspace Management Polish & Connection Environment Coloring (`ui-dev` & `core-dev`) ✅
- [x] แก้ไขหน้า Manage Workspaces Modal ให้ทำงานได้สมบูรณ์ 100%:
  - รองรับการสร้าง Workspace และ Project Folder เปล่า และ persist เก็บใน `workspace_layout.json`
  - มี Section แสดงรายการ Workspaces & Project Folders พร้อมปุ่ม Rename และ Delete
  - รายการ Dropdown สำหรับ Assign Connection ดึงจาก Workspaces & Folders ทั้งหมดแบบ real-time
  - เพิ่ม Modals ยืนยัน/เปลี่ยนชื่อ: `RenameWorkspace`, `RenameProjectFolder`, `DeleteWorkspace`, `DeleteProjectFolder`, `QuickNewFolder`
- [x] เพิ่มช่อง Dropdown เลือก Workspace / Folder ในหน้าต่าง Add / Edit Connection Modal โดยตรง:
  - สลับเลือก Workspace (หรือ Standalone) และเลือก Folder ใน Workspace นั้นได้ทันทีตั้งแต่ตอนสร้าง Connection ใหม่
  - Pre-populate ข้อมูลเดิมเมื่อเปิด Edit Connection Modal
  - กำหนด Workspace & Folder อัตโนมัติเมื่อกดสร้าง Connection จาก Context menu
- [x] เพิ่ม Context Menu ตอนคลิกขวาที่ Workspace และ Folder ใน Left Panel:
  - Right-click Workspace: `Add Connection...` (fixed workspace), `New Folder...`, `Rename Workspace...`, `Delete Workspace...`
  - Right-click Folder: `Add Connection...` (fixed workspace & folder), `Rename Folder...`, `Delete Folder...`
- [x] เปลี่ยน Tag Env ให้เป็นสีที่ Connection แทน:
  - ยกเลิก Badge กล่องสี่เหลี่ยม text `[DEV]` / `[TEST]` / `[PROD]` ที่กินพื้นที่ Sidebar แนวนอน
  - แสดงเป็นสีที่ตัวชื่อ Connection โดยตรง (DEV = เขียว, TEST = ส้ม/เหลือง, PROD = แดง) พร้อมสัญลักษณ์จุดสี `● `
  - แสดงชื่อ Environment Tag ใน Tooltip เมื่อ Hover อย่างชัดเจน
- [x] QA Verification:
  - เพิ่ม Unit Tests ใน `tests/core_workspace_tests.rs` ครบ 123/123 tests ผ่าน (48 lib + 26 core workspace + 31 database + 11 query + 7 ui theme)
  - `cargo clippy --all-targets -- -D warnings` (0 warnings)
  - `cargo fmt --check` (สะอาด 100%)

