# Forge — מפרט פרוטוקול תקשורת (Wire Protocol Specification)

## סקירה כללית (Overview)

צומתי Forge מחליפים הודעות בקרה בסריאליזציית bincode מעל חיבורי QUIC מוצפנים שהוקמו על ידי Iroh. טנזורי הפעלה (Activation tensors) מועברים כבייטים גולמיים בתוך הודעות `Forward`. מימוש v1 הנוכחי משתמש באותה מעטפת עבור הסקת seed/requester מקומית ועבור הודעות צינור (pipeline) רב-קפיצות עתידיות.

## מעטפת הודעה (Message Envelope)

כל הודעה עטופה במעטפת:

```rust
pub struct Envelope {
    pub msg_id: u64,
    pub sender: NodeId,
    pub timestamp: u64, // unix millis
    pub payload: Payload,
}
```

כללי אימות הנאכפים על ידי זמן הריצה הנוכחי:
- `Envelope.sender` חייב להתאים לזהות העמית המרוחק המאומת מחיבור ה-QUIC.
- ערכי `msg_id` כפולים מאותו עמית נזרקים בתוך חלון שידור חוזר (replay window) מוגבל.
- `Hello.capability.node_id` ו-`Welcome.capability.node_id` חייבים להתאים ל-`Envelope.sender`.
- טווחי שכבות פגומים ואורכי טנזור לא תואמים נדחים לפני שמטפלים ברמה גבוהה יותר רואים את ההודעה.
- שדות ה-prompt והטוקנים מוגבלים (`prompt_text` ו-`max_tokens`) כך שעמית אחד לא יכול לבקש מאחר להקצות עבודה בלתי מוגבלת.

## Payload Enum

```rust
pub enum Payload {
    Hello(Hello),
    Welcome(Welcome),
    AssignShard(AssignShard),
    ShardReady(ShardReady),
    PipelineTopology(PipelineTopologyMsg),
    Forward(Forward),
    TokenResult(TokenResult),
    InferenceRequest(InferenceRequest),
    TokenStream(TokenStreamMsg),
    Error(ErrorMsg),
    Heartbeat(Heartbeat),
    Ping(Ping),
    Pong(Pong),
    Leaving(Leaving),
    Rebalance(Rebalance),
}
```

## גילוי ולחיצת יד (Discovery and Handshake)

```rust
pub struct Hello {
    pub version: u16,
    pub capability: PeerCapability,
}

pub struct Welcome {
    pub version: u16,
    pub capability: PeerCapability,
    pub known_peers: Vec<PeerInfo>,
}

pub struct PeerInfo {
    pub node_id: NodeId,
    pub addr: String,
}
```

- `version` היא גרסת הפרוטוקול המפורסמת על ידי השולח.
- `capability` מתארת CPU, זיכרון, רוחב פס ואזור לצורך החלטות תזמון.
- `known_peers` היא רשימת עמיתים אופורטוניסטית, ולא רישום סמכותי גלובלי.

## הקצאת שיתוף (Shard Assignment)

הודעות אלו מגדירות את צינור שכבות הרב-קפיצות העתידי. הן חלק מ-v1 למרות שמימוש הייחוס הנוכחי מריץ בעיקר הסקת מודל שלם על ה-seed.

```rust
pub struct AssignShard {
    pub model_id: ModelId,
    pub model_source: String,
    pub layer_range: LayerRange,
    pub pipeline_position: u8,
    pub upstream: Option<NodeId>,
    pub downstream: Option<NodeId>,
}

pub struct ShardReady {
    pub model_id: ModelId,
    pub layer_range: LayerRange,
    pub load_time_ms: u64,
}

pub struct PipelineTopologyMsg {
    pub model_id: ModelId,
    pub stages: Vec<PipelineStage>,
}
```

## הודעות הסקה (Inference Messages)

### Forward

הודעת `Forward` נושאת טנזור הפעלה בין שלבי צינור.

```rust
pub struct Forward {
    pub request_id: u64,
    pub sequence_pos: u32,
    pub tensor_meta: TensorMeta,
    #[serde(with = "serde_bytes")]
    pub tensor_data: Vec<u8>,
}

pub struct TensorMeta {
    pub shape: Vec<u32>,
    pub dtype: DType,
    pub byte_len: u32,
}
```

- `tensor_data` הם בייטים גולמיים של הפעלה.
- `dtype` הוא אחד מ-`F16`, `F32` או `I8`.
- תעבורת WAN צפויה להעדיף ייצוגים קומפקטיים כמו `I8`.

### TokenResult

הודעת `TokenResult` שמורה עבור מזהי טוקנים שנדגמו בשלב הסופי בהסקה רב-קפיצתית.

```rust
pub struct TokenResult {
    pub request_id: u64,
    pub tokens: Vec<u32>,
}
```

### InferenceRequest

זרימת ה-seed/requester הנוכחית שולחת טקסט prompt ישירות. ה-seed מבצע טוקניזציה מקומית.

```rust
pub struct InferenceRequest {
    pub request_id: u64,
    pub prompt_text: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
}
```

- `prompt_text` מחליף את מזהי הטוקנים הקודמים.
- `max_tokens` הוא גם מגבלת ייצור וגם בסיס לבדיקות יכולת תשלום CU מראש.

### TokenStreamMsg

תגובת ההזרמה הנוכחית שולחת קטעי טקסט מפוענחים במקום מזהי טוקנים.

```rust
pub struct TokenStreamMsg {
    pub request_id: u64,
    pub text: String,
    pub is_final: bool,
}
```

- `text` הוא קטע טקסט מפוענח המתאים לתצוגה מיידית.
- `is_final = true` סוגר את הזרם עבור הבקשה.

### ErrorMsg

כשלים ברמת הבקשה מוחזרים כשגיאות מובנות במקום קטעי טקסט.

```rust
pub enum ErrorCode {
    InvalidRequest,
    InsufficientBalance,
    Busy,
    Internal,
}

pub struct ErrorMsg {
    pub request_id: u64,
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}
```

- `request_id` קושר את השגיאה לבקשת ההסקה הפעילה.
- `retryable` אומר לקורא האם ניסיון חוזר מאוחר יותר הוא הגיוני.
- ה-seed/runtime הנוכחי משתמש בזה עבור בקשות לא תקינות, דחיית CU, עומס יתר וכשלי ייצור.

## בריאות וחיוניות (Health and Liveness)

```rust
pub struct Heartbeat {
    pub uptime_sec: u64,
    pub load: f32,
    pub memory_free_gb: f32,
    pub battery_pct: Option<u8>,
}

pub struct Ping {
    pub sent_at: u64,
}

pub struct Pong {
    pub ping_sent_at: u64,
    pub received_at: u64,
}
```

## ניהול צביר (Cluster Management)

```rust
pub enum LeaveReason {
    Shutdown,
    LowBattery,
    UserRequest,
}

pub struct Leaving {
    pub reason: LeaveReason,
    pub drain_time_ms: u64,
}

pub enum RebalanceReason {
    NodeJoined,
    NodeLeft,
    ModelUpgrade,
}

pub struct Rebalance {
    pub new_topology: PipelineTopologyMsg,
    pub reason: RebalanceReason,
}
```

## חתימת עסקאות (הוכחת עבודה מועילה)

Forge משתמש בעסקאות חתומות כפולה כדי להוכיח שמחשוב בוצע והתקבל. גם הספק וגם הצרכן חייבים לחתום על אותם בייטים קנוניים של העסקה.

### TradeProposal

נשלח על ידי הספק לאחר סיום ההסקה. מכיל את פרטי העסקה ואת חתימת ה-Ed25519 של הספק.

```rust
pub struct TradeProposal {
    pub request_id: u64,
    pub provider: NodeId,
    pub consumer: NodeId,
    pub cu_amount: u64,
    pub tokens_processed: u64,
    pub timestamp: u64,
    pub model_id: String,
    pub provider_sig: Vec<u8>,  // חתימת Ed25519 באורך 64 בייטים
}
```

### TradeAccept

נשלח על ידי הצרכן כדי לחתום חתימה נגדית על העסקה.

```rust
pub struct TradeAccept {
    pub request_id: u64,
    pub consumer_sig: Vec<u8>,  // חתימת Ed25519 באורך 64 בייטים
}
```

### TradeGossip

מופץ לכל העמיתים המחוברים לאחר רישום עסקה חתומה כפולה. כל צומת יכול לאמת את שתי החתימות.

```rust
pub struct TradeGossip {
    pub provider: NodeId,
    pub consumer: NodeId,
    pub cu_amount: u64,
    pub tokens_processed: u64,
    pub timestamp: u64,
    pub model_id: String,
    pub provider_sig: Vec<u8>,
    pub consumer_sig: Vec<u8>,
}
```

### בייטים קנוניים לחתימה (Canonical Bytes for Signing)

שני הצדדים חותמים על אותו ייצוג בינארי דטרמיניסטי:

```
provider_id (32 bytes) + consumer_id (32 bytes) +
cu_amount (8 bytes LE) + tokens_processed (8 bytes LE) +
timestamp (8 bytes LE) + model_id (variable bytes)
```

### זרימת חתימה כפולה (Dual-Sign Flow)

```text
ספק (Seed)                                צרכן (Worker)
    |                                         |
    |--- TokenStream (הסקה) ----------------->|
    |--- TokenStream (סופי) ----------------->|
    |                                         |
    |--- TradeProposal (provider_sig) ------->|
    |                                         |
    |    [הצרכן מאמת את provider_sig]          |
    |    [הצרכן חותם חתימה נגדית]              |
    |                                         |
    |<--- TradeAccept (consumer_sig) ---------|
    |                                         |
    [הספק מאמת את שתי החתימות]                 |
    [רושם SignedTradeRecord בספר]             |
    [מפיץ TradeGossip לרשת]                   |
```

אם הצרכן אינו מגיב תוך 5 שניות, הספק חוזר לרישום עסקה לא חתומה (תאימות לאחור).

### הפצת Gossip

כשצומת מקבל הודעת `TradeGossip`:
1. אימות שתי חתימות ה-Ed25519.
2. בדיקת כפילות SHA-256 (דחייה אם כבר נראה).
3. רישום העסקה בספר החשבונות המקומי.
4. העסקה אינה מופצת מחדש (Gossip של קפיצה אחת למניעת עומס).

## כללי סריאליזציה (Serialization Rules)

- הודעות בקרה משתמשות ב-bincode.
- `Forward.tensor_data` מועבר כבייטים רציפים גולמיים.
- המעטפת (envelope) נשארת אחידה בכל סוגי ההודעות כדי שהתעבורה תישאר גנרית.
- זמן הריצה דוחה פריימים של הפרוטוקול הגדולים מ-64 MiB.
- הפרוטוקול אינו כולל שדות סליקה של פיאט, בלוקצ'יין או בורסה. אלו שייכים לאינטגרציות מחוץ לפרוטוקול.

## מחזור חיי חיבור

### זרימת seed/requester נוכחית

```text
מבקש (Requester)                         Seed
  |                                      |
  |--- QUIC + הצפנה -------------------->|
  |--- Hello --------------------------->|
  |<-- Welcome --------------------------|
  |--- InferenceRequest ---------------->|
  |                                      | [CU מוזמן]
  |<-- TokenStreamMsg ------------------ |
  |<-- TokenStreamMsg ------------------ |
  |<-- TokenStreamMsg (סופי) ---------- |
  |<-- TradeProposal ------------------- | [הספק חותם]
  |--- TradeAccept --------------------->| [הצרכן חותם חתימה נגדית]
  |                                      | [SignedTradeRecord נרשם]
  |                                      | [TradeGossip מופץ]
```

### זרימת רב-קפיצות עתידית

```text
מתאם             עובד א'         עובד ב'        שלב סופי
    |                |               |                |
    |-- AssignShard->|               |                |
    |-- AssignShard----------------->|               |                |
    |-- AssignShard---------------------------------->|
    |<-- ShardReady--|               |                |
    |<---------------- ShardReady ---|                |
    |<-------------------------------- ShardReady ---|
    |-- הפצת PipelineTopology לכולם ----------------->|
    |-- Forward ---->|-- Forward ---->|-- TokenResult->|
```

## גרסאות (Versioning)

גרסה נוכחית: `1`

- עמיתים מפרסמים את הגרסה שלהם דרך `Hello` ו-`Welcome`.
- מימוש הייחוס הנוכחי מניח עמיתים תואמים ומתעלם מנתונים עתידיים לא מוכרים.
- שינויים מהותיים בפרוטוקול צריכים להעלות את ה-`version` ולהגדיר התנהגות נסיגה (downgrade) במפורש.
