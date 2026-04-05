# Forge — مواصفات بروتوكول المراسلة (Wire Protocol Specification)

## نظرة عامة (Overview)

تتبادل عقد Forge رسائل تحكم متسلسلة باستخدام bincode عبر اتصالات QUIC مشفرة تم إنشاؤها بواسطة Iroh. يتم نقل تنسورات التنشيط (Activation tensors) كبايتات خام داخل رسائل `Forward`. يستخدم التنفيذ الحالي v1 نفس الغلاف لاستنتاج seed/requester المحلي ولرسائل خط الأنابيب (pipeline) متعددة القفزات المستقبلية.

## غلاف الرسالة (Message Envelope)

يتم تغليف كل رسالة في غلاف:

```rust
pub struct Envelope {
    pub msg_id: u64,
    pub sender: NodeId,
    pub timestamp: u64, // unix millis
    pub payload: Payload,
}
```

قواعد التحقق التي يفرضها وقت التشغيل الحالي:
- يجب أن يتطابق `Envelope.sender` مع هوية الند البعيد الموثقة من اتصال QUIC.
- يتم تجاهل قيم `msg_id` المكررة من نفس الند ضمن نافذة إعادة تشغيل محدودة (replay window).
- يجب أن يتطابق `Hello.capability.node_id` و `Welcome.capability.node_id` مع `Envelope.sender`.
- يتم رفض نطاقات الطبقات المشوهة وأطوال التنسورات غير المتطابقة قبل أن ترى المعالجات ذات المستوى الأعلى الرسالة.
- حقول المطالبة (prompt) والتوكن محدودة (`prompt_text` و `max_tokens`) بحيث لا يمكن لند أن يطلب من آخر تخصيص عمل غير محدود.

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

## الاكتشاف والمصافحة (Discovery and Handshake)

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

- `version` هو إصدار البروتوكول الذي يعلن عنه المرسل.
- تصف `capability` وحدة المعالجة المركزية، الذاكرة، عرض النطاق الترددي، والمنطقة لاتخاذ قرارات الجدولة.
- `known_peers` هي قائمة أقران انتهازية، وليست سجلاً موثوقاً عالمياً.

## تعيين التجزئة (Shard Assignment)

تحدد هذه الرسائل خط أنابيب الطبقات متعدد القفزات المستقبلي. وهي جزء من v1 على الرغم من أن التنفيذ المرجعي الحالي يقوم بتشغيل استنتاج النموذج بالكامل على الـ seed.

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

## رسائل الاستنتاج (Inference Messages)

### Forward

تحمل رسالة `Forward` تنسور تنشيط بين مراحل خط الأنابيب.

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

- `tensor_data` هي بايتات التنشيط الخام.
- `dtype` هو واحد من `F16` أو `F32` أو `I8`.
- يتوقع أن يفضل النقل عبر الشبكة الواسعة (WAN) التمثيلات المدمجة مثل `I8`.

### TokenResult

رسالة `TokenResult` محجوزة لمعرفات التوكنات التي تم أخذ عينات منها في المرحلة النهائية في الاستنتاج متعدد القفزات.

```rust
pub struct TokenResult {
    pub request_id: u64,
    pub tokens: Vec<u32>,
}
```

### InferenceRequest

يرسل تدفق seed/requester الحالي نص المطالبة مباشرة. يقوم الـ seed بالتحويل إلى توكنات (tokenization) محلياً.

```rust
pub struct InferenceRequest {
    pub request_id: u64,
    pub prompt_text: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
}
```

- يحل `prompt_text` محل نظام معرفات التوكنات السابق.
- `max_tokens` هو حد للتوليد وأساس لفحوصات القدرة على تحمل تكلفة CU المسبقة.

### TokenStreamMsg

ترسل استجابة التدفق الحالية أجزاء نصية مفكوكة بدلاً من معرفات التوكنات.

```rust
pub struct TokenStreamMsg {
    pub request_id: u64,
    pub text: String,
    pub is_final: bool,
}
```

- `text` هو جزء نصي مفكوك مناسب للعرض الفوري.
- `is_final = true` يغلق التدفق الخاص بالطلب.

### ErrorMsg

يتم إرجاع حالات الفشل المتعلقة بالطلبات كأخطاء مصنفة بدلاً من أجزاء نصية محملة بشكل زائد.

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

- يربط `request_id` الخطأ بطلب الاستنتاج النشط.
- يخبر `retryable` المتصل ما إذا كان من المنطقي إعادة المحاولة لاحقاً.
- يستخدم الـ seed/runtime الحالي هذا للطلبات غير الصالحة، رفض CU، تشبع التزامن، وفشل التوليد.

## الصحة والحيوية (Health and Liveness)

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

## إدارة العنقود (Cluster Management)

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

## توقيع الصفقات (إثبات العمل المفيد)

يستخدم Forge صفقات موقعة مزدوجاً لإثبات إجراء الحوسبة واستلامها. يجب على كل من المزود والمستهلك توقيع نفس بايتات الصفقة المعيارية.

### TradeProposal

يرسلها المزود بعد اكتمال الاستنتاج. تحتوي على تفاصيل الصفقة وتوقيع Ed25519 الخاص بالمزود.

```rust
pub struct TradeProposal {
    pub request_id: u64,
    pub provider: NodeId,
    pub consumer: NodeId,
    pub cu_amount: u64,
    pub tokens_processed: u64,
    pub timestamp: u64,
    pub model_id: String,
    pub provider_sig: Vec<u8>,  // توقيع Ed25519 بطول 64 بايت
}
```

### TradeAccept

يرسلها المستهلك للتوقيع المقابل على الصفقة.

```rust
pub struct TradeAccept {
    pub request_id: u64,
    pub consumer_sig: Vec<u8>,  // توقيع Ed25519 بطول 64 بايت
}
```

### TradeGossip

تُبث إلى جميع الأقران المتصلين بعد تسجيل صفقة موقعة مزدوجاً. يمكن لأي عقدة التحقق من كلا التوقيعين.

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

### البايتات المعيارية للتوقيع (Canonical Bytes for Signing)

يوقع كلا الطرفين نفس التمثيل الثنائي الحتمي:

```
provider_id (32 bytes) + consumer_id (32 bytes) +
cu_amount (8 bytes LE) + tokens_processed (8 bytes LE) +
timestamp (8 bytes LE) + model_id (variable bytes)
```

### تدفق التوقيع المزدوج (Dual-Sign Flow)

```text
المزود (Seed)                             المستهلك (Worker)
    |                                         |
    |--- TokenStream (استنتاج) -------------->|
    |--- TokenStream (نهائي) ---------------->|
    |                                         |
    |--- TradeProposal (provider_sig) ------->|
    |                                         |
    |    [المستهلك يتحقق من provider_sig]      |
    |    [المستهلك يوقع توقيعاً مقابلاً]         |
    |                                         |
    |<--- TradeAccept (consumer_sig) ---------|
    |                                         |
    [المزود يتحقق من كلا التوقيعين]             |
    [يسجل SignedTradeRecord في دفتر الحسابات]  |
    [يبث TradeGossip للشبكة]                  |
```

إذا لم يستجب المستهلك خلال 5 ثوانٍ، يعود المزود لتسجيل صفقة غير موقعة (متوافق مع الإصدارات السابقة).

### انتشار الـ Gossip

عندما تستلم عقدة رسالة `TradeGossip`:
1. التحقق من كلا توقيعي Ed25519.
2. فحص إلغاء التكرار SHA-256 (رفض إذا تمت رؤيته مسبقاً).
3. تسجيل الصفقة في دفتر الحسابات المحلي.
4. لا تتم إعادة بث الصفقة (Gossip قفزة واحدة لمنع العواصف).

## قواعد التسلسل (Serialization Rules)

- تستخدم رسائل التحكم bincode.
- يتم نقل `Forward.tensor_data` كبايتات متجاورة خام.
- يظل الغلاف (envelope) موحداً عبر جميع أنواع الرسائل لتبقى وسائط النقل عامة.
- يرفض وقت التشغيل إطارات البروتوكول التي تزيد عن 64 ميجابايت.
- لا يتضمن البروتوكول حقول تسوية للنقد أو البلوكشين أو البورصة. هذه تنتمي لتكاملات خارج البروتوكول.

## دورة حياة الاتصال

### تدفق seed/requester الحالي

```text
طالب الخدمة (Requester)                   الـ Seed
  |                                      |
  |--- QUIC + تشفير -------------------->|
  |--- Hello --------------------------->|
  |<-- Welcome --------------------------|
  |--- InferenceRequest ---------------->|
  |                                      | [حجز CU]
  |<-- TokenStreamMsg ------------------ |
  |<-- TokenStreamMsg ------------------ |
  |<-- TokenStreamMsg (نهائي) ---------- |
  |<-- TradeProposal ------------------- | [المزود يوقع]
  |--- TradeAccept --------------------->| [المستهلك يوقع مقابلاً]
  |                                      | [تسجيل SignedTradeRecord]
  |                                      | [بث TradeGossip]
```

### تدفق متعدد القفزات المستقبلي

```text
المنسق           العامل أ         العامل ب        المرحلة النهائية
    |                |               |                |
    |-- AssignShard->|               |                |
    |-- AssignShard----------------->|               |                |
    |-- AssignShard---------------------------------->|
    |<-- ShardReady--|               |                |
    |<---------------- ShardReady ---|                |
    |<-------------------------------- ShardReady ---|
    |-- بث PipelineTopology للجميع ------------------>|
    |-- Forward ---->|-- Forward ---->|-- TokenResult->|
```

## الإصدارات (Versioning)

الإصدار الحالي: `1`

- يعلن الأقران عن إصدارهم عبر `Hello` و `Welcome`.
- يفترض التنفيذ المرجعي الحالي الأقران المتوافقين ويتجاهل الحمولات المستقبلية غير المعروفة.
- التغييرات الكبيرة في المراسلة يجب أن تزيد الـ `version` وتحدد سلوك الرجوع لإصدار أقدم (downgrade) بشكل صريح.
