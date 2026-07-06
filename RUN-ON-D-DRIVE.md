# تشغيل البوت على قرص D (Windows) — 24/7

## ⚠️ تحذير
- Selfbot يخالف شروط Discord — استخدم حساب تجريبي
- لا تشغّل نسختين بنفس TOKEN (سحابة + جهازك = OK إذا حسابين مختلفين، نفس الحساب = مشكلة)

---

## التثبيت على `D:\discord-selfbot-ai`

### 1) المتطلبات
- [Rust](https://rustup.rs/)
- [Git](https://git-scm.com/download/win)

### 2) التثبيت (مرة واحدة)

افتح **CMD** أو **PowerShell** كمسؤول (اختياري) وشغّل:

```bat
git clone -b cursor/discord-selfbot-arabic-ai-bd43 https://github.com/NLj0/bot-Discord-Rust.git D:\discord-selfbot-ai
cd /d D:\discord-selfbot-ai
windows\install-d-drive.bat
```

أو إذا المشروع موجود عندك:
```bat
cd /d D:\discord-selfbot-ai
windows\install-d-drive.bat
```

### 3) إعداد `.env`

افتح `D:\discord-selfbot-ai\.env`:

```env
USER_TOKEN=your_token_here
SERVER_ID=766953289409232896
CHANNEL_ID=1040405511545294988
DATA_DIR=data
BACKFILL=0
BACKFILL_LIMIT=1000
BACKFILL_ONLY=0
LIVE_POLL_SECS=2
```

- `BACKFILL=0` → لايف فقط (للتشغيل الطويل 1–2 يوم)
- `BACKFILL=1` → يسحب 1000 رسالة أولاً ثم يكمل لايف

---

## التشغيل 24/7 (1–2 يوم أو أكثر)

### طريقة 1: نافذة CMD (تشوف اللوق)
```bat
cd /d D:\discord-selfbot-ai
run-live-forever.bat
```
- يعيد التشغيل تلقائياً إذا انقطع
- اللوق: `D:\discord-selfbot-ai\data\live_run.log`

### طريقة 2: خلفية بدون نافذة (مُفضّل للتشغيل الطويل)
```bat
cd /d D:\discord-selfbot-ai
start-hidden.vbs
```
- دuble-click على `start-hidden.vbs`

---

## أوامر مهمة

| الأمر | الوظيفة |
|---|---|
| `run-live-forever.bat` | تشغيل لايف + إعادة تشغيل تلقائية |
| `start-hidden.vbs` | نفس الشيء بدون نافذة |
| `stop-bot.bat` | إيقاف البوت |
| `status-bot.bat` | هل شغال؟ + آخر إحصائيات |

---

## أين تُحفظ البيانات؟

```
D:\discord-selfbot-ai\data\
├── clean\
│   ├── messages.jsonl
│   ├── training_pairs.jsonl
│   └── training_export.jsonl
├── state\
├── reports\
│   └── latest.json
└── live_run.log
```

---

## تصدير بيانات التدريب

```bat
cd /d D:\discord-selfbot-ai
target\release\export_training.exe
```

---

## مسار مخصص على D:

عدّل السطر الأول في `windows\install-d-drive.bat`:
```bat
set "INSTALL_DIR=D:\your-folder-name"
```

---

## نصائح للتشغيل 1–2 يوم

1. **Settings → Power** → Sleep = **Never** (لا تنام الشاشة/الجهاز)
2. **Ethernet** أفضل من WiFi
3. لا تفتح Discord على نفس الحساب من المتصفح كثيراً
4. كل يوم: `status-bot.bat` للتأكد
5. لا تشغّل نسختين — `stop-bot.bat` قبل أي تشغيل جديد

---

**Updated:** 2026-07-06
