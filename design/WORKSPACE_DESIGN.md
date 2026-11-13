# Mongo Studio: Workspace & Connection Left Panel Design (Rev. 2)

> **สถานะ:** เสนอการออกแบบฉบับปรับปรุงตามภาพอ้างอิงจริง (Reference-Aligned Design)  
> **ไฟล์ HTML ตัวอย่างต้นแบบ:** [design/workspace_left_panel.html](file:///Users/worldinfinity/Desktop/claude/mongodb-admin-tools/design/workspace_left_panel.html)  
> **วันที่:** 13 พฤศจิกายน 2026

---

## 1. การปรับปรุงตามข้อเสนอแนะของผู้ใช้และภาพอ้างอิง

1. **ตัด Chip Filter Environment ออก:**  
   ไม่ใช้ชิปตัวกรองด้านบนเพื่อให้หน้าจอสะอาด ไม่รก โดยการระบุ Environment จะใช้แท็กขนาดกะทัดรัดนำหน้าชื่อ Connection (เช่น `DEV`, `TEST`, `PROD`) และผู้ใช้สามารถพิมพ์ `-dev`, `-prod`, หรือ `-test` ในช่อง Search เพื่อกรองได้โดยตรง
2. **ปรับ Layout ให้ตรงตามภาพอ้างอิง (Screenshots):**  
   * **ด้านบน:** Search bar พร้อม shortcut `⌘ K`
   * **Quick Access:** เมนู `⭐ Favorites` (พร้อมตัวเลขนับ) และ `🕒 Recent`
   * **หัวข้อ WORKSPACES:** มี Dropdown สลับดู Workspace เฉพาะเจาะจง หรือดูแบบ All Workspaces
   * **Tree View:** แสดงตามโครงสร้าง `Customer` (Workspace) $\rightarrow$ `Project` (Folder) $\rightarrow$ `Connection`
   * **Empty State:** เมื่อยังไม่มี Workspace จะแสดงหน้าต่างแนะนำพร้อมปุ่ม **Manage Workspaces** เหมือนภาพอ้างอิง
3. **แก้ปัญหาสับสนปุ่ม `+ Connection` และ `+ Folder`:**  
   ออกแบบใหม่เพื่อไม่ให้ผู้ใช้สับสนเรื่องลำดับการกด (ดูรายละเอียดในข้อ 3)
4. **คงปุ่ม Config และ Manage Workspaces (รวมถึง Import / Export):**  
   มีปุ่ม `⚙️ Manage Workspaces` ที่ Footer ด้านล่าง และปุ่ม Quick Config/Export ที่ Header สำหรับสำรองข้อมูล นำเข้าไฟล์ JSON จาก MongoDB Compass หรือแชร์ให้ทีม

---

## 2. โครงสร้าง UI ซ้ายมือ (Left Panel Architecture)

```
┌──────────────────────────────────────────────┐
│ 🗄️ MongoDB Client                       ⚙️ ⎘ │
├──────────────────────────────────────────────┤
│ 🔍 Search connections...                 ⌘ K │
├──────────────────────────────────────────────┤
│ ⭐ Favorites                              4  │
│ 🕒 Recent                                    │
├──────────────────────────────────────────────┤
│ WORKSPACES ▾                              +  │
│                                              │
│ ▼ 🏢 Customer A                           +  │
│   ▼ 📁 ERP                                +  │
│       DEV  erp-dev                           │
│     [ TEST erp-test ] (Selected/Active)      │
│       PROD erp-prod                          │
│   ▶ 📁 E-Commerce                            │
│                                              │
│ ▶ 🏢 Customer B                            3 │
│ ▶ 📁 Internal                              2 │
├──────────────────────────────────────────────┤
│ ⚙️ Manage Workspaces                      📥 │
└──────────────────────────────────────────────┘
```

---

## 3. วิธีแก้ปัญหาเรื่องปุ่ม `+ Connection` และ `+ Folder` (UX Clarity)

### ปัญหาเดิม:
การวางปุ่ม `[+ Connection]` และ `[+ Folder]` ไว้คู่กันด้านบน ทำให้เกิดคำถามว่า:
* *ต้องกดอะไรก่อน?*
* *ถ้ายังไม่มีโฟลเดอร์แล้วกด + Connection ข้อมูลจะไปอยู่ที่ไหน?*
* *ถ้ากดผิดลำดับจะทำให้ต้องมาเสียเวลาจัดกลุ่มใหม่หรือไม่?*

### แนวทางแก้ไขในดีไซน์นี้ (3 เสาหลัก):
1. **ปุ่ม `+` แบบ Context Menu รวมที่หัวข้อ WORKSPACES:**  
   บนแถบ Workspaces มีปุ่ม `+` เพียงปุ่มเดียว เมื่อกดจะมีเมนูปรากฏเรียงตามลำดับความบ่อย:
   * `➕ New Connection...` (งานที่ทำบ่อยที่สุด)
   * `📁 New Project / Folder...`
   * `🏢 New Workspace / Customer...`
2. **ปุ่ม `+` แบบ In-line Hover บนตัว Node โดยตรง:**  
   * เมื่อเลื่อนเมาส์ชี้ที่ `Customer A` จะมีปุ่ม `+` เล็กๆ ปรากฏขึ้นเพื่อ **"สร้าง Folder ใน Customer A ทันที"**
   * เมื่อชี้ที่ `ERP` จะมีปุ่ม `+` ปรากฏขึ้นเพื่อ **"สร้าง Connection ใน ERP ทันที"**  
   *ผู้ใช้จึงไม่ต้องกังวลเรื่องการเลือก Context เพราะระบบผูกให้โดยอัตโนมัติ*
3. **ฟอร์มการสร้าง Connection แบบยืดหยุ่น (Non-blocking):**  
   ในหน้าต่าง Add Connection ช่องเลือก **Workspace** และ **Folder** จะเป็นแบบ **Optional** (เลือกเป็น `None` เพื่อสร้างเป็น Standalone ได้ หรือเลือกสร้างใหม่ในฟอร์มได้ทันที) ผู้ใช้ที่ต้องการทดสอบด่วนสามารถกรอก URI แล้วกด Connect ได้เลยโดยไม่ต้องสร้าง Folder ก่อน

---

## 4. ระบบ Config, Manage Workspaces และ Import/Export

ปุ่ม **`⚙️ Manage Workspaces`** ถูกจัดไว้ที่ด้านล่างสุดของ Left Panel และมีปุ่ม Quick Export/Import ที่ Header:
* **Export Workspaces (JSON):** ส่งออกโครงสร้างลูกค้า, โฟลเดอร์ และ connection configurations สำหรับนำไปเปิดใช้งานบนเครื่องอื่น (ระบบจะตัดรหัสผ่านออกเพื่อความปลอดภัย)
* **Import Connections:** นำเข้า connection จากไฟล์ JSON หรือจาก MongoDB URI string
* **Reorder & Organize:** ลากจัดลำดับความสำคัญของ Workspace และ Project Folder ได้อย่างอิสระ

---

## 5. การเปิดดูตัวอย่าง

* **HTML Mockup:** [design/workspace_left_panel.html](file:///Users/worldinfinity/Desktop/claude/mongodb-admin-tools/design/workspace_left_panel.html)
  *(สามารถกดสลับโหมด "Tree View" และ "Empty State" ที่มุมบนขวา รวมถึงทดสอบปุ่ม `+`, ค้นหา `⌘K`, และเปิดหน้าต่าง Manage Workspaces & Import/Export ได้จริง)*
