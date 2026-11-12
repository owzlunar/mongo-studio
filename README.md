# Mongo Studio — Project Architecture & Development Plan

โปรเจกต์สำหรับพัฒนา MongoDB Desktop Admin Tool ด้วย Rust โดยออกแบบ UI ให้มีหน้าตาคล้าย Visual Studio Code แต่เน้นการจัดการ MongoDB ไม่ใช่ IDE สำหรับเขียนโค้ด

เป้าหมายคือสร้าง Desktop Application ที่รองรับ Windows, Linux และ macOS โดยไม่ใช้ JavaScript, HTML หรือ CSS

---

## 1. Technology Stack

| Technology | หน้าที่ |
| :--- | :--- |
| `Rust` | ภาษาโปรแกรมหลัก |
| `egui` | GUI Framework |
| `eframe` | Desktop Window Framework |
| `egui_dock` | Tab, Docking และ Split Panel |
| `egui_extras` | Table และ UI Components |
| `mongodb` | MongoDB Rust Driver |
| `bson` | จัดการ BSON Documents |
| `tokio` | Async Runtime |
| `serde` | Serialization / Deserialization |
| `serde_json` | JSON Processing |
| `syntect` | Syntax Highlighting |
| `rfd` | Native File Dialog |
| `keyring` | จัดเก็บ Credentials ใน OS Keychain |
| `directories` | จัดการ Application Directories |
| `tracing` | Logging |
| `anyhow` | Error Handling |
| `thiserror` | Custom Error Types |
| `uuid` | Unique ID |

---

## 2. Application UI Design

ออกแบบ UI ให้คล้าย VS Code โดยใช้ Dark Theme เป็นค่าเริ่มต้น

### Main Layout

```text
┌────────────────────────────────────────────────────────────┐
│ Mongo Studio             File  Edit  View  Help            │
├────┬───────────────────┬───────────────────────────────────┤
│    │                   │                                   │
│ A  │ CONNECTIONS       │  users ×  products ×              │
│ C  │                   │                                   │
│ T  │ ▼ MongoDB Local   │  Documents  Indexes  Schema        │
│ I  │   ▼ admin         │                                   │
│ V  │     users         │  Filter                           │
│ I  │     products      │  {"status": "active"}              │
│ T  │     orders        │                                   │
│ Y  │                   │  ┌─────────────────────────────┐  │
│    │ ▼ shop_db         │  │ _id  name  email  status    │  │
│ B  │     customers     │  │ 1    Alice ...   active     │  │
│ A  │     payments      │  │ 2    Bob   ...   active     │  │
│ R  │                   │  └─────────────────────────────┘  │
│    │ + Add Connection  │                                   │
├────┴───────────────────┼───────────────────────────────────┤
│                       │ TERMINAL / OUTPUT / PROBLEMS       │
│                       │                                   │
├───────────────────────┴───────────────────────────────────┤
│ ● Connected     localhost:27017             MongoDB       │
└───────────────────────────────────────────────────────────┘
```

### UI Components

| Component | หน้าที่ |
| :--- | :--- |
| **Activity Bar** | Navigation Icons |
| **Sidebar** | Connection Explorer |
| **Tree View** | Database / Collection Tree |
| **Tab Bar** | เปิดหลาย Collection พร้อมกัน |
| **Main Workspace** | แสดงข้อมูลและเครื่องมือ |
| **Bottom Panel** | Output, Logs, Query History |
| **Status Bar** | Connection Status และข้อมูลระบบ |
| **Command Palette** | ค้นหาคำสั่ง |
| **Theme** | Dark / Light Theme |

---

## 3. Project Structure

```text
mongo-studio/
│
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
│
├── .vscode/
│   ├── extensions.json
│   ├── settings.json
│   └── launch.json
│
├── .agents/
│   └── skills/
│       └── rust-skills/
│           ├── SKILL.md
│           ├── rules/
│           └── checks/
│
├── assets/
│   ├── fonts/
│   ├── icons/
│   └── themes/
│
├── src/
│   ├── main.rs
│   ├── app.rs
│   │
│   ├── core/
│   │   ├── mod.rs
│   │   ├── state.rs
│   │   ├── events.rs
│   │   ├── commands.rs
│   │   └── error.rs
│   │
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── layout.rs
│   │   ├── activity_bar.rs
│   │   ├── sidebar.rs
│   │   ├── status_bar.rs
│   │   ├── tab_bar.rs
│   │   ├── command_palette.rs
│   │   │
│   │   └── components/
│   │       ├── buttons.rs
│   │       ├── inputs.rs
│   │       ├── tree_view.rs
│   │       ├── data_grid.rs
│   │       └── json_viewer.rs
│   │
│   ├── database/
│   │   ├── mod.rs
│   │   ├── connection.rs
│   │   ├── connection_manager.rs
│   │   ├── database_service.rs
│   │   ├── collection_service.rs
│   │   ├── document_service.rs
│   │   └── index_service.rs
│   │
│   ├── query/
│   │   ├── mod.rs
│   │   ├── filter.rs
│   │   ├── projection.rs
│   │   ├── sorting.rs
│   │   └── aggregation.rs
│   │
│   ├── models/
│   │   ├── mod.rs
│   │   ├── connection.rs
│   │   ├── document.rs
│   │   ├── collection.rs
│   │   └── database.rs
│   │
│   ├── workspace/
│   │   ├── mod.rs
│   │   ├── tabs.rs
│   │   ├── panels.rs
│   │   └── persistence.rs
│   │
│   └── theme/
│       ├── mod.rs
│       ├── dark.rs
│       └── light.rs
│
└── tests/
    ├── database_tests.rs
    └── query_tests.rs
```

### Module Responsibilities

| Module | หน้าที่ |
| :--- | :--- |
| `core` | Application State, Events และ Commands |
| `ui` | UI Components ทั้งหมด |
| `database` | MongoDB Connection และ Database Operations |
| `query` | Query Filter, Sorting และ Aggregation |
| `models` | Data Structures |
| `workspace` | Tabs, Panels และ Layout Persistence |
| `theme` | Dark / Light Theme |

> **หลักการสำคัญ:** UI ต้องไม่เรียก MongoDB Driver โดยตรง แต่เรียกผ่าน Service Layer เพื่อแยก UI Logic ออกจาก Database Logic

---

## 4. MongoDB Features

### Connection Management
- Multiple MongoDB Connections
- MongoDB URI
- Authentication
- Connection Profiles
- Connection Status
- Saved Connections
- Connection Pool Management
- Secure Credential Storage

### Database Explorer
- List Databases
- List Collections
- Collection Statistics
- Collection Information
- Create Database
- Drop Database
- Create Collection
- Drop Collection

### Document Management
- Browse Documents
- View Document
- Insert Document
- Edit Document
- Delete Document
- JSON View
- JSON Tree View
- Pagination
- Sorting
- Filtering

### Query Editor
- MongoDB Filter
- Projection
- Sorting
- Limit
- Skip
- Query History
- Saved Queries

### Aggregation
- Aggregation Pipeline Builder
- Add / Remove Stages
- Pipeline Preview
- Execute Pipeline
- JSON Pipeline Editor

### Index Management
- List Indexes
- Create Index
- Drop Index
- View Index Information

### Schema Explorer
- Field Analysis
- Data Type Analysis
- Schema Visualization
- Field Frequency
- Sample Documents

---

## 5. VS Code Extensions

Extensions สำหรับพัฒนา Rust ใน Visual Studio Code

| Extension | Extension ID | ความสำคัญ |
| :--- | :--- | :--- |
| **rust-analyzer** | `rust-lang.rust-analyzer` | จำเป็น |
| **CodeLLDB** | `vadimcn.vscode-lldb` | Debugging |
| **Even Better TOML** | `tamasfe.even-better-toml` | Cargo.toml |
| **crates** | `serayuzgur.crates` | Dependencies |
| **Error Lens** | `usernamehw.errorlens` | Error Visualization |
| **GitLens** | `eamodio.gitlens` | Git Management |
| **TODO Tree** | `gruntfuggly.todo-tree` | TODO Management |
| **Material Icon Theme** | `PKief.material-icon-theme` | File Icons |

### `.vscode/extensions.json`

```json
{
  "recommendations": [
    "rust-lang.rust-analyzer",
    "vadimcn.vscode-lldb",
    "tamasfe.even-better-toml",
    "serayuzgur.crates",
    "usernamehw.errorlens",
    "eamodio.gitlens",
    "gruntfuggly.todo-tree",
    "PKief.material-icon-theme"
  ]
}
```

---

## 6. Cargo Dependencies

ติดตั้ง Dependencies เริ่มต้น:

```bash
cargo add eframe egui_dock egui_extras
cargo add mongodb bson
cargo add tokio --features full
cargo add serde --features derive
cargo add serde_json
cargo add anyhow thiserror
cargo add tracing tracing-subscriber
cargo add rfd keyring directories uuid
```

ตรวจสอบ Dependency Versions ด้วย:

```bash
cargo tree -d
```

> **ข้อแนะนำ:** ควรตรวจสอบความเข้ากันได้ระหว่าง `eframe`, `egui` และ `egui_dock` ก่อนเริ่มพัฒนา

---

## 7. Development Roadmap

แบ่งการพัฒนาออกเป็น 5 Phases

### Phase 1 — UI Framework *(เริ่มต้นก่อน)*
- Desktop Window
- Dark Theme
- Activity Bar
- Sidebar
- Tab System
- Workspace
- Bottom Panel
- Status Bar

### Phase 2 — MongoDB Connection
- MongoDB Driver
- Connection Manager
- Multiple Connections
- Authentication
- Connection Profiles
- Secure Credential Storage

### Phase 3 — Database Explorer & Admin User Management
- Database Tree & Collection Tree (Fast Concurrent Lazy-Loading)
- Refresh Explorer & On-Demand Collection Introspection
- Collection Information, Indexes & Schema Statistics
- Admin User & Role Management (`Users` Node, Right-Click Create/Delete User, Database Role Assignment, Change Password, Protected `admin` Lock)

### Phase 4 — Document Manager
- Document Browser
- JSON Viewer
- Insert / Update / Delete
- Filter
- Sort
- Pagination

### Phase 5 — Advanced Features
- Aggregation Builder
- Index Management
- Schema Explorer
- Query History
- Saved Queries
- Export / Import

---

## 8. Architecture Principles

สิ่งสำคัญที่ควรออกแบบตั้งแต่ต้น:

- **Separation of Concerns:** แยก UI, Database และ Business Logic
- **Async Database Operations:** ใช้ Tokio เพื่อไม่ให้ UI Thread ค้าง
- **Connection Management:** แต่ละ Connection มี Client และ Lifecycle ที่เหมาะสม
- **Secure Storage:** เก็บ Password ผ่าน OS Keychain ไม่เก็บ Plain Text
- **Error Handling:** ใช้ `Result` และ Custom Error Types
- **Data Safety:** มี Confirmation สำหรับ Delete และ Bulk Update
- **Cross Platform:** ทดสอบ Windows, Linux และ macOS
- **Scalability:** แยก Module ให้สามารถเพิ่ม Feature ได้ง่าย

---

## 9. Initial Setup

สร้างโปรเจกต์:

```bash
cargo new mongo-studio
cd mongo-studio
code .
```

### Project Configuration

| รายการ | ค่า |
| :--- | :--- |
| **Project Name** | Mongo Studio |
| **Language** | Rust |
| **GUI Framework** | egui + eframe |
| **Docking** | egui_dock |
| **Database** | MongoDB |
| **Async Runtime** | Tokio |
| **Development IDE** | Visual Studio Code |
| **UI Style** | VS Code Inspired |
| **Default Theme** | Dark |
| **Platforms** | Windows / Linux / macOS |
| **JavaScript** | ไม่ใช้ |
| **HTML / CSS** | ไม่ใช้ |

---

## 10. Initial Development Goal

เป้าหมายแรกคือสร้าง Mongo Studio UI Shell โดยยังไม่ต้องเชื่อมต่อ MongoDB

### สิ่งที่ต้องทำให้เสร็จ:
- [x] Desktop Window
- [x] Custom Dark Theme
- [x] Activity Bar
- [x] Connections Sidebar
- [x] Database Tree View (ใช้ Mock Data)
- [x] Tab System
- [x] Main Workspace
- [x] Bottom Panel
- [x] Status Bar
- [x] Window Layout Persistence

เมื่อ UI Shell เสร็จแล้ว จึงเริ่ม Phase 2 เพื่อเชื่อมต่อ MongoDB จริง

---

### ข้อสรุป
- ใช้ **Rust + egui + eframe + egui_dock + mongodb + tokio** เป็น Technology Stack หลัก โดยเริ่มจาก Modular Architecture ภายใน Cargo Project เดียวก่อน และค่อยแยกเป็นหลาย Crates เมื่อโปรเจกต์เติบโต
- เป้าหมายคือสร้าง MongoDB Desktop Admin Tool ที่มี UX แบบ VS Code แต่มีฟังก์ชันการจัดการฐานข้อมูลโดยเฉพาะ รองรับทั้ง 3 Desktop Platforms และไม่พึ่งพา JavaScript เลย
