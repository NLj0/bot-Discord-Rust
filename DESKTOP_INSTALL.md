# 💻 تثبيت Selfbot على الديسكتوب

دليل شامل لتثبيت وتشغيل الـ Selfbot على جهازك الشخصي (Windows/Linux/Mac)

---

## 📋 المتطلبات

قبل البدء، تأكد من تثبيت:

### 1. **Rust** (إلزامي)
```bash
# Windows (PowerShell)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Linux/Mac
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

أو حمّل من: https://rustup.rs/

### 2. **Git** (إلزامي)
- Windows: https://git-scm.com/download/win
- Linux: `sudo apt install git`
- Mac: `brew install git`

### 3. **OpenSSL** (مطلوب على Linux)
```bash
# Ubuntu/Debian
sudo apt update
sudo apt install pkg-config libssl-dev

# Fedora/RHEL
sudo dnf install pkg-config openssl-devel

# Arch
sudo pacman -S pkg-config openssl
```

---

## 📦 طريقة التثبيت

### الطريقة 1: التثبيت التلقائي (الأسهل) ⭐

#### على **Windows**:
```powershell
# افتح PowerShell واكتب:
cd Desktop
git clone https://github.com/NLj0/bot-Discord-Rust.git
cd bot-Discord-Rust
git checkout cursor/discord-selfbot-arabic-ai-bd43

# شغّل ملف التثبيت
.\setup-windows.bat
```

#### على **Linux/Mac**:
```bash
# افتح Terminal واكتب:
cd ~/Desktop
git clone https://github.com/NLj0/bot-Discord-Rust.git
cd bot-Discord-Rust
git checkout cursor/discord-selfbot-arabic-ai-bd43

# شغّل ملف التثبيت
chmod +x setup-linux.sh
./setup-linux.sh
```

---

### الطريقة 2: التثبيت اليدوي

#### 1. نزّل المشروع
```bash
cd Desktop
git clone https://github.com/NLj0/bot-Discord-Rust.git
cd bot-Discord-Rust
git checkout cursor/discord-selfbot-arabic-ai-bd43
```

#### 2. أنشئ ملف `.env`
```bash
# انسخ ملف المثال
cp .env.example .env

# على Windows
copy .env.example .env
```

#### 3. احصل على Discord Token

**طريقة 1: Discord Web (الأسهل)**
1. افتح https://discord.com/app في المتصفح
2. اضغط `F12` لفتح Developer Tools
3. اذهب لـ **Console**
4. الصق:
```javascript
(webpackChunkdiscord_app.push([[''],{},e=>{m=[];for(let c in e.c)m.push(e.c[c])}]),m).find(m=>m?.exports?.default?.getToken!==void 0).exports.default.getToken()
```
5. انسخ الـ Token

**طريقة 2: Discord Desktop + BetterDiscord**
- ثبّت BetterDiscord
- استخدم Console

#### 4. عدّل ملف `.env`
افتح `.env` بأي محرر نصوص وضع الـ Token:
```env
USER_TOKEN=your_token_here
```

#### 5. بناء البرنامج
```bash
# للتطوير (أسرع)
cargo build --bin selfbot

# للإنتاج (أسرع في التشغيل)
cargo build --release --bin selfbot
```

---

## 🚀 التشغيل

### تشغيل مباشر (Development)
```bash
cargo run --bin selfbot
```

### تشغيل النسخة المحسّنة (Release)
```bash
# Windows
.\target\release\selfbot.exe

# Linux/Mac
./target/release/selfbot
```

### إنشاء اختصار على سطح المكتب

#### Windows:
1. اذهب لـ `target\release\`
2. انقر يمين على `selfbot.exe`
3. اختر "إنشاء اختصار"
4. انقل الاختصار لسطح المكتب
5. اختياري: غيّر الأيقونة

#### Linux (Ubuntu/GNOME):
أنشئ ملف `selfbot.desktop`:
```bash
nano ~/.local/share/applications/selfbot.desktop
```

محتوى الملف:
```ini
[Desktop Entry]
Version=1.0
Type=Application
Name=Discord Selfbot
Comment=Arabic AI Discord Selfbot
Exec=/home/YOUR_USERNAME/Desktop/bot-Discord-Rust/target/release/selfbot
Icon=utilities-terminal
Terminal=true
Categories=Utility;
```

حفظ: `Ctrl+O`, ثم `Enter`, ثم `Ctrl+X`

#### Mac:
استخدم Automator لإنشاء Application:
1. افتح Automator
2. New → Application
3. أضف "Run Shell Script"
4. اكتب:
```bash
cd ~/Desktop/bot-Discord-Rust
./target/release/selfbot
```
5. احفظ كـ `Selfbot.app`

---

## 📊 الناتج المتوقع

عند التشغيل الناجح:
```
🚀 ============================================================
⚙️  تهيئة Selfbot...
============================================================
✅ Token تم تحميله
✅ Intents تم تهيئتها
✅ Client جاهز
🔄 جاري الاتصال بـ Discord...

🚀 ============================================================
✅ Selfbot متصل بنجاح!
👤 الحساب: YourUsername
🆔 ID: 123456789
📡 جاهز لقراءة الرسائل...
============================================================
```

عند إرسال رسالة:
```
============================================================
📨 رسالة جديدة
============================================================
👤 المرسل: أحمد (ID: 123456789)
📍 القناة: 987654321
💬 المحتوى:
   "السلام عليكم"
============================================================
```

---

## 🛠️ حل المشاكل الشائعة

### المشكلة 1: "cargo: command not found"
**السبب**: Rust غير مثبت أو PATH غير محدث  
**الحل**:
```bash
# أعد فتح Terminal بعد تثبيت Rust
# أو شغّل:
source $HOME/.cargo/env
```

### المشكلة 2: "error: failed to compile openssl-sys"
**السبب**: OpenSSL غير مثبت (Linux فقط)  
**الحل**:
```bash
sudo apt install pkg-config libssl-dev
```

### المشكلة 3: "USER_TOKEN غير موجود"
**السبب**: ملف `.env` غير موجود أو فارغ  
**الحل**:
```bash
cp .env.example .env
# ثم عدّل .env وضع Token
```

### المشكلة 4: "Invalid token"
**السبب**: Token خاطئ أو منتهي  
**الحل**:
- احصل على token جديد من Discord
- تأكد من نسخ Token كامل بدون مسافات
- تأكد من عدم وجود علامات اقتباس حول Token

### المشكلة 5: "Connection failed"
**السبب**: شبكة أو حساب محظور  
**الحل**:
- تحقق من الاتصال بالإنترنت
- جرب VPN
- تحقق من حالة حساب Discord
- استخدم حساب تجريبي

### المشكلة 6: البرنامج يتوقف فجأة
**السبب**: Discord اكتشف الـ selfbot  
**الحل**:
- استخدم حساب آخر
- قلل معدل النشاط
- لا تستخدم على حساب مهم

---

## 🔐 نصائح الأمان

### ✅ افعل:
1. ✅ استخدم حساب تجريبي فقط
2. ✅ احتفظ بـ `.env` سري
3. ✅ لا تشارك Token مع أحد
4. ✅ استخدم في سيرفرات خاصة
5. ✅ توقف فوراً لو شكيت في حظر

### ❌ لا تفعل:
1. ❌ لا تستخدم على حسابك الرئيسي
2. ❌ لا تشارك الكود مع Token فيه
3. ❌ لا ترفع `.env` على GitHub
4. ❌ لا تستخدم في سيرفرات عامة كبيرة
5. ❌ لا تسوي spam أو automated messages

---

## 📂 هيكل المشروع

```
Desktop/
└── bot-Discord-Rust/
    ├── src/
    │   ├── selfbot.rs       ← الكود الرئيسي
    │   └── main.rs          ← Bot العادي
    ├── target/
    │   ├── debug/
    │   │   └── selfbot      ← نسخة التطوير
    │   └── release/
    │       └── selfbot      ← نسخة محسّنة ⭐
    ├── .env                 ← إعداداتك (أنشئه)
    ├── .env.example         ← مثال
    ├── Cargo.toml           ← Dependencies
    ├── SELFBOT_README.md    ← دليل تفصيلي
    ├── QUICKSTART.md        ← بداية سريعة
    └── DESKTOP_INSTALL.md   ← هذا الملف
```

---

## 🎯 استخدام يومي

### سيناريو 1: اختبار سريع
```bash
cd ~/Desktop/bot-Discord-Rust
cargo run --bin selfbot
# اختبر بإرسال رسائل
# Ctrl+C للإيقاف
```

### سيناريو 2: تشغيل دائم
```bash
cd ~/Desktop/bot-Discord-Rust
./target/release/selfbot > logs.txt 2>&1 &
# يشتغل في الخلفية ويحفظ logs
```

### سيناريو 3: مع Terminal منفصل
```bash
# افتح Terminal جديد
cd ~/Desktop/bot-Discord-Rust
./target/release/selfbot
# خلّيه مفتوح وشوف الرسائل
```

---

## 📊 معلومات تقنية

### حجم الملفات:
```
selfbot (debug):   ~15 MB
selfbot (release):  ~5 MB
Dependencies:     ~500 MB (أول بناء)
```

### استهلاك الموارد:
```
RAM:  < 50 MB
CPU:  < 1% (idle)
CPU:  5-10% (نشط)
```

### سرعة البناء:
```
أول بناء:  10-15 دقيقة
بناء تالي:  10-30 ثانية
```

---

## 🔄 التحديثات

لتحديث الكود:
```bash
cd ~/Desktop/bot-Discord-Rust
git pull origin cursor/discord-selfbot-arabic-ai-bd43
cargo build --release --bin selfbot
```

---

## 📞 الدعم

### المشاكل التقنية:
1. راجع [SELFBOT_README.md](./SELFBOT_README.md)
2. راجع [QUICKSTART.md](./QUICKSTART.md)
3. تحقق من logs
4. شغّل بـ `RUST_LOG=debug` لمزيد من التفاصيل

### الأوامر المفيدة:
```bash
# مزيد من التفاصيل
RUST_LOG=debug cargo run --bin selfbot

# تنظيف البناء
cargo clean

# تحديث Dependencies
cargo update

# فحص الأخطاء
cargo check --bin selfbot
```

---

## ⚖️ إخلاء المسؤولية

- ❌ استخدام Selfbot يخالف Discord ToS
- ⚠️ للأغراض التعليمية فقط
- 🔒 الاستخدام على مسؤوليتك
- 📚 نحن غير مسؤولين عن أي حظر

---

## ✨ الخطوات القادمة

بعد التثبيت والاختبار:
1. ✅ شغّل واختبر قراءة الرسائل
2. ⏳ المرحلة التالية: حفظ البيانات
3. ⏳ بناء نظام التنظيف التلقائي
4. ⏳ التدريب التدريجي
5. ⏳ نموذج الـ 3B

---

**تم التحديث**: 2026-07-05  
**الحالة**: ✅ جاهز للتثبيت على الديسكتوب
