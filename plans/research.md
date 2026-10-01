# Brews Bingo — Hosting Research

- **Research date:** September 30, 2026
- **Status:** Research and recommendations only — no hosting provider or architecture selected.
- **Related documents:** [Business requirements](requirements.md) · [High-level design](hld.md)
- **Purpose:** Compare free and free-tier hosting for the web frontend and required backend, with particular attention to Vercel and AWS. Findings are intended to inform the HLD; implementation detail belongs in the subsequent `lld.md`.

Prices below are in USD, before applicable taxes. Prices and quotas can change; verify them again when creating the production account. Quoted allowances are selected decision-relevant limits, not complete service specifications. Provider facts have numbered sources; project-specific confirmations identify their source, and recommendations and design implications are assessments, not provider guarantees.

## 1. Overview and preliminary recommendation

**Start by evaluating Cloudflare for a zero-subscription-cost design**, provided a serverless backend is acceptable. Its static-asset delivery is free and unlimited, while Workers and SQLite-backed Durable Objects offer recurring free compute/storage allowances.[35][5][8]
Durable Objects support live connections, but free dynamic-service limits still constrain usage and can interrupt operation when exhausted.[36][8]

**Include Vercel Hobby as an eligible free-hosting candidate.** Eligibility for the stated entertainment-only use is confirmed for project planning per the user's instruction. Brews Bingo will not advertise Brews products or sell anything. Compare Hobby on technical fit, usage limits, and full-stack cost rather than retaining an open eligibility question.

**Vercel has usable free persistent-database options.** Neon and Supabase both offer $0 plans through Vercel Marketplace.[63][67] The fair comparison is Cloudflare's free storage versus **Vercel Hobby plus a selected free Postgres plan**, not Cloudflare storage versus no Vercel database. Database quotas are provider-specific and remain separate from Vercel Function allowances.[65][68]

**AWS is a credible low-cost option, not a verified forever-free full stack.** There are recurring Lambda and DynamoDB allowances and a distinct CloudFront Free delivery plan, but a new AWS Free account itself expires.[47][50][53]
Real-time transport, storage requests, and other components can still be chargeable.[54][58]

**A traditional backend process changes the shortlist.** Oracle Always Free provides actual virtual machines, but capacity shortages and idle reclamation make it a risk for an intermittently used venue app. Render Free is convenient for a prototype, but Render explicitly advises against using free instances for production.[29][13]

**Hosting alone does not satisfy offline gameplay.** A cloud provider cannot deliver new host draws to isolated player devices without a reachable communication path. The HLD must separately resolve local state, host authority, venue-network operation, claim validation, and reconnection.

### What “backend server” means in this comparison

The requirement for a backend remains intact. Two implementation models are evaluated, not selected:

- **Conventional server:** A persistent application process in a VM/container, such as an API plus a WebSocket server.
- **Serverless/managed backend:** Server-side application logic and persistent data supplied by functions, stateful managed components, or a backend platform. This still supplies backend responsibilities, but not necessarily a continuously running process under our control.

If a conventional process is mandatory, serverless-only candidates need to be excluded or supplemented. A static frontend by itself does **not** meet the backend requirement, and a database by itself does not implement the game's authorization and validation rules.

## 2. Evaluation criteria

The project-specific priorities are:

1. **Permitted-use suitability:** Use the confirmed project context: entertainment within Brews, with no advertising of Brews products or sales. Vercel Hobby eligibility is a resolved planning input supplied by the user; other providers' eligibility findings remain as documented.
2. **Durable game state:** Draws, cards, and completed-game history must survive refreshes, restarts, and deployments (BR-013, BR-021, BR-026).
3. **Correct live updates:** Host, audience, and players need consistent draw order and valid claims (BR-002, BR-007, BR-018, BR-027).
4. **Outage continuity:** Temporary internet loss must not silently invalidate BR-016 or D-005.
5. **Cost behavior:** Distinguish hard-cap free plans, recurring allowances with paid overages, expiring credits, and expiring databases.
6. **Event reliability:** Sleeping services, paused databases, quota exhaustion, and reconnection are more important than a nominal $0 label.
7. **Platform coverage:** The web frontend must serve desktop/mobile clients; the same backend should be usable by Android/iOS clients. Native-app packaging and store distribution are separate from hosting.
8. **Maintainability:** Compare server administration, provider-specific code, backup/export, and migration effort.

No attendance, concurrency, traffic, or retention forecast has been approved. Therefore, this report does not promise that any complete production deployment will cost $0.

## 3. At-a-glance comparison

| Provider / offering | Web frontend | Backend model | Meaning of “free” and main caveat | Preliminary fit |
| --- | --- | --- | --- | --- |
| **Cloudflare Workers / Pages** | Static assets or Pages | Workers; Durable Objects; optional D1 | Recurring free quotas; static requests free; dynamic operations can fail at limits.[35][5][8] | Leading free-tier candidate if serverless is acceptable. |
| **Vercel** | Static and framework-based frontend | Functions; WebSockets currently Beta; Marketplace Postgres | Hobby plus Neon Free or Supabase Free can provide persistent SQL storage without a paid database subscription; provider quotas apply separately.[1][63][67] | Eligible free candidate per the user's confirmation; compare complete storage and realtime options, not an assumed lack of free persistence. |
| **AWS** | CloudFront/S3 or Amplify | Lambda/API Gateway/DynamoDB, or EC2 | Mix of ongoing allowances, paid meters, and temporary credits; AWS Free account expires.[47][50][53] | Strong low-cost alternative, greater billing/configuration complexity. |
| **Netlify** | CDN frontend and deployment workflow | Functions and managed data offerings; assess live-update service separately | 300 recurring credits/month, hard cap; sites pause on exhaustion.[9][11] | Viable frontend candidate; shared credit budget needs attention. |
| **Render** | Free static sites | Conventional web service | 750 instance-hours/workspace/month; idle sleep; free Postgres expires after 30 days.[13] | Prototype rather than live-event production backend. |
| **Supabase** | Pair with a frontend host | Managed Postgres/API, Edge Functions, Realtime | Recurring free plan; limited connections/messages and inactivity pause.[17][20] | Useful managed-backend candidate, subject to event-size and availability checks. |
| **Firebase / Google Cloud** | Firebase Hosting | Functions or Cloud Run plus Firestore/other storage | Spark has free limits; custom compute needs billing-enabled services, with free allowances then charges.[21][23][38] | Useful cross-platform candidate if billing exposure is acceptable. |
| **Oracle Cloud** | VM-hosted site or separate CDN frontend | Conventional VM | Always Free resources, subject to capacity and idle reclamation.[29] | Best investigated as a free VM option, not assumed reliable capacity. |
| **Railway** | App service | App/container service | Trial followed by $1 recurring monthly credit.[32] | Small allowance; prototype or low-cost fallback, not assumed sufficient. |
| **Fly.io** | App service | VM-based app | Current new-user trial: 2 VM hours or 7 days, whichever comes first.[37] | Not an ongoing free-hosting shortlist choice. |

## 4. Provider findings: overview, pros, and cons

### 4.1 Cloudflare — strongest initial free-tier candidate

**Overview.** A possible arrangement is a static web app on Workers Static Assets or Pages, with Workers for API logic.[35][27][5]
SQLite-backed Durable Objects are a candidate for coordinated game state and live connections.[8][36]
D1 is another storage option to evaluate, not an additional required database.[5]

**Selected free limits**

- Workers: **100,000 requests/day** and **10 ms CPU time per invocation** on Free. Static-asset requests are free and unlimited; requests that invoke Worker code, including dynamic rendering, use the compute allowance.[5][35]
- Durable Objects: **100,000 requests/day**, **13,000 GB-seconds/day** of duration, and SQLite storage allowances of **5 million rows read/day**, **100,000 rows written/day**, and **5 GB total storage**. Only SQLite-backed Durable Objects are available on Free; excess operations fail rather than receiving unlimited capacity.[8]
- D1: **5 million rows read/day**, **100,000 rows written/day**, and **5 GB total storage** on Free. Rows scanned matter, not just API-call count.[5]
- Pages: **500 builds/month**, one concurrent build, **20,000 files**, and **25 MiB maximum per asset** on Free. Pages Functions consume Workers quotas.[27]
- Durable Objects support WebSockets; their Hibernation API allows idle objects to sleep without disconnecting clients.[36]

**Pros**

- Static frontend delivery and a stateful live-update backend are available within the same platform's free offerings.[35][8][36]
- Hibernation is a useful fit for bingo's pauses between draws, rather than keeping a conventional server busy throughout the event.[36]
- Assessment: a per-game coordination component could simplify ordered draws and duplicate prevention. This is a proposed design direction, not an approved HLD decision.

**Cons**

- Free CPU, request, duration, and storage limits must all be respected; a small request count alone does not establish eligibility for $0 operation.[5][8]
- Hard limits protect cost but can interrupt the game.[8]
- Assessment: provider-specific runtime/storage APIs introduce migration work; this is not a general-purpose always-running VM.
- No explicit personal-only restriction was found in the reviewed developer-plan documentation, but that is **not** a legal/commercial eligibility guarantee. Review current service terms and acceptable-use rules before deployment.

**Brews Bingo fit:** First candidate to evaluate if serverless satisfies the backend requirement. Verify card generation and winner-validation CPU usage, draw concurrency, message fan-out, recovery, and offline behavior before selecting it.

### 4.2 Vercel — eligible free candidate for entertainment-only use

**Overview.** Vercel supplies frontend deployment/CDN capabilities and server-side Functions. The current documentation now lists **WebSocket support in Beta on all plans**; older advice that Vercel Functions cannot serve WebSockets is no longer consistent with the retrieved docs.[40][25]

Vercel also provisions databases through its Marketplace, including Postgres from Neon and Supabase, with integrated management and billing.[62]
The former **Vercel Postgres** product is no longer available: existing databases moved to Neon in December 2024, and new projects use Marketplace integrations.[61]
This is a product/ownership distinction, **not a lack of free durable storage**: both integrations list plans starting at $0.[63][67]

**Confirmed project context**

- **Intended purpose:** Entertainment within Brews.
- **Excluded activity:** Advertising Brews products or selling anything through the app.
- **Eligibility status:** Confirmed for project planning, per the user's instruction. This records the user's confirmation, not independent verification or a direct approval from Vercel.
- **Research treatment:** Hobby is included in the free-hosting shortlist. Eligibility is resolved and is no longer a decision gate or an open research action.

**General policy reference:** Vercel restricts Hobby to non-commercial personal use.[2] This policy remains documented for context; it is not an unresolved item for the confirmed project use.

**Selected pricing and limits**

- Hobby is free for **non-commercial personal use**, with listed included usage of **100 GB fast data transfer** and **1 million Function invocations**. Evaluate these allowances against the planned workload.[1][2]
- Pro is advertised at **$20/month**, with $20 included usage credit; confirm the full account configuration and usage charges before purchase.[40]
- WebSocket connections close when a Function reaches its maximum duration. Reconnection and state restoration are required, and new connections need not reach the same instance.[25]

#### Free persistent Postgres options — verified September 30, 2026

These are alternative database selections, not allowances that automatically stack together or come from Vercel Function quotas. Neon's native integration provides access to Neon plans with billing through Vercel; Supabase describes Marketplace-created projects as functioning like directly created projects, with billing through Vercel and a separate billing cycle.[63][68] Select the **Free** database plan explicitly when provisioning; no account or resource has been created for this research.

| Limit / behavior | Neon Free via Vercel | Supabase Free via Vercel |
| --- | --- | --- |
| **Persistent SQL storage** | **0.5 GB per project**; data remains stored while compute is suspended.[70] | **500 MB database per project**, separate from its **1 GB file-storage** allowance.[17] |
| **Database compute / queries** | **100 CU-hours per project/month**; scales up to **2 CU (approximately 8 GB RAM)**. All branch computes share the project's compute allowance.[70][65] | Shared CPU and **500 MB RAM**; **unlimited API requests** is the published request allowance, not unlimited throughput.[17] |
| **Data transfer** | **5 GB public network transfer per project/month**.[70] | **5 GB egress plus 5 GB cached egress** in the Free plan; cached delivery is not an extra general SQL-transfer allowance.[17] |
| **Idle behavior** | Compute scales to zero after **5 minutes of inactivity**; this cannot be disabled on Free. Stored data persists.[65][70] | Free projects pause after **one week of inactivity**; **two active projects** allowed.[17] |
| **Exhaustion behavior** | Used-up compute or transfer suspends the project's compute until the next billing period or an upgrade. Storage-growing writes fail above the storage cap. No Free-plan overage billing.[70] | Exceeding **500 MB database size** puts the database into read-only mode; organization-level fair-use restrictions are also documented.[72] |
| **Recovery** | Free instant-restore window: **6 hours**, capped at **1 GB of change history**; not a substitute for the project's backup/export plan.[65] | Automatic backups and point-in-time recovery are **not included** in Free.[17] |
| **Connections / live updates** | Pooling supports up to **10,000 client connections**, not 10,000 active queries or TPS. Database compute determines practical throughput.[69] | Realtime includes **200 peak connections**, **2 million messages/month**, and **100 messages/sec**. These are Realtime limits, not SQL TPS.[17][20] |

**Neon compute illustration, not a forecast:** `100 CU-hours ÷ compute size` permits **400 active hours at 0.25 CU**, **100 hours at 1 CU**, or **50 hours at 2 CU**, before other branch compute usage. These values were calculated programmatically; the allowance measures compute size multiplied by runtime, not a number of queries.[65] No fixed SQL queries/day or guaranteed TPS entitlement was established by the reviewed Neon/Supabase plan documentation. Benchmark the chosen queries, indexes, transactions, and connection pool rather than treating a connection count or “unlimited API requests” as a throughput guarantee.[69][17]

**Rust suitability and authorization:** Neon documents Rust connections using `postgres` and `tokio-postgres` with TLS; a TypeScript database client is not required.[71] Engineering recommendation: keep privileged connection strings in the Rust backend, never in a Dioxus browser/mobile build. Validate the selected runtime's driver/TLS compatibility and use bounded connection pools; this research has not exercised a deployment.

**Persistence and realtime are separate decisions.** A free Postgres database can persist cards, draws, claims, and history, but it does not by itself implement reliable cross-instance broadcasts. Vercel's WebSocket documentation requires shared state/coordination outside a Function instance.[25]
Neon's transaction-mode pooler restricts session-dependent features such as `LISTEN`/`NOTIFY`, so do not assume it is a drop-in message bus.[69]
Supabase Realtime is a separate managed option available alongside Postgres, with the explicit limits above.[67][20]
If clients connect directly to Supabase Realtime, Vercel's Function connection-duration limit is not the lifecycle of those connections; Supabase's own limits apply. The app must still authorize subscriptions and winning claims.

#### Corrected storage comparison for the three shortlisted platforms

| Stack | Ongoing free persistent storage | Relevant quota distinction |
| --- | --- | --- |
| **Cloudflare Workers + SQLite-backed Durable Objects** | **5 GB total**; **5 million rows read/day**, **100,000 rows written/day**.[8] | Storage and compute/duration are separate allowances; this is SQLite within Durable Objects, not Postgres. |
| **Vercel Hobby + Neon Free** | **0.5 GB Postgres/project**, **100 CU-hours/project/month**, **5 GB public transfer/project/month**.[70] | Real free SQL persistence; compute/transfer exhaustion and idle suspension must be planned for. |
| **Vercel Hobby + Supabase Free** | **500 MB Postgres/project**, with optional managed Realtime and separate file storage.[17] | Database size, egress, project pauses, and Realtime limits are separate constraints. |
| **AWS Lambda + provisioned DynamoDB** | **25 GB**, **25 provisioned RCUs**, **25 provisioned WCUs**.[52][53] | NoSQL capacity model, not Postgres; recurring database allowance does not remove AWS account-plan expiry or paid-overage exposure.[47] |

**Assessment:** Free persistence is **not** a reason to downgrade Vercel. Vercel Hobby + Neon Free is a concrete SQL-backed candidate; Vercel Hobby + Supabase Free adds a separately limited realtime option. Cloudflare remains worth evaluating for integrated per-game coordination and hibernating WebSockets, not because Vercel lacks a free database. No provider or architecture is selected by this correction.

**Pros**

- Integrated deployment workflow, preview/development tooling, CDN, and Functions are useful if the eventual frontend framework fits the platform.[40][1]
- Native WebSockets are now a technical option, subject to Beta status and lifecycle constraints.[25]
- Free hosting and Functions make Hobby a candidate without a Pro subscription.[1]
- Free Postgres integrations make persistent cards, game history, and transactional data feasible without a paid database subscription, within the selected plan's limits.[63][67]

**Cons**

- Free-tier usage limits still apply; in most cases, exceeding a Hobby allowance requires waiting until 30 days have passed before using the affected feature again.[1]
- WebSocket Beta and bounded connection lifetime add risk and recovery requirements for a live event.[25]
- Persistent game state must not rely on one Function instance's memory; the docs explicitly warn that new connections can reach different instances.[25]
- Database compute/storage/transfer limits and suspension behavior must be monitored separately from Hobby Function quotas. Marketplace billing integration does not merge those allowances.[70][68]

**Brews Bingo fit:** Include **Hobby + Neon Free** and **Hobby + Supabase Free** as concrete free-persistence candidates for the confirmed entertainment-only use. Evaluate the Rust backend, chosen database, realtime transport, and offline design together. A paid Vercel upgrade and a paid database upgrade are separate decisions; neither is assumed necessary just to obtain persistent storage. Eligibility confirmation does not select Vercel or approve an architecture.

### 4.3 AWS — detailed account and service distinctions

**Overview.** A candidate stack is CloudFront/S3 for the web frontend, Lambda for game APIs, and DynamoDB for durable records.[50][51][52]
API Gateway WebSockets can supply push updates; Amplify offers an integrated hosting/development path, and EC2 provides the conventional-server alternative.[54][55][56]
These are options to compare, not an approved architecture.

#### Account-level Free plan versus ongoing service allowances

For new accounts under the post–July 15, 2025 model, AWS provides **$100 signup credit plus up to $100 additional earned credit** and a choice of Free or Paid account plan.[56][60]

- The **Free account plan ends after six months or credit exhaustion**, whichever occurs first. AWS then closes the account and access to resources/data is lost unless the account is upgraded; this is not a permanent production hosting plan.[47]
- Credits have a separate **12-month expiry from account creation**. Upgrading to Paid can retain eligible unused credits, but usage beyond credits/free allowances becomes billable.[47][60]
- Existing or past AWS customers may be ineligible for the new-customer Free plan/credits. Actual eligibility cannot be determined without the owner's account history.[47]
- AWS's FAQ explicitly welcomes entrepreneurs and small businesses; no general personal-only exclusion was identified in these Free Tier materials.[47]

#### Service-level findings

| Service | Verified offer / behavior | Implication |
| --- | --- | --- |
| **CloudFront flat-rate Free** | $0/month, **1 million requests**, **100 GB transfer**, and **5 GB S3 Standard storage credits/month**; up to three Free plans/account. AWS explicitly places this offering under a **Paid AWS account plan**.[48][49][50] | Useful free frontend-delivery allowance, separate from the temporary Free account. Not a free backend bundle. |
| **CloudFront limits** | No delivery-overage charges under the flat-rate offer, but allowances are not hard limits and sustained excessive use can affect delivery. S3 request charges are separate.[48][58] | “No overages” for CloudFront must not be generalized to the whole AWS account. |
| **Lambda** | Recurring **1 million requests + 400,000 GB-seconds/month**; the free tier does not apply to Provisioned Concurrency.[51][53] | Can cover small API workloads; duration and extra services still matter. |
| **DynamoDB** | Recurring **25 GB storage + 25 provisioned write capacity units + 25 provisioned read capacity units**, using the Standard table class/provisioned capacity.[52][53] | Persistent game/card/history option. Do not assume on-demand request billing is included in the same free capacity offer. |
| **API Gateway** | HTTP APIs meter requests; WebSocket APIs meter sent/received messages and connection-minutes, with messages charged in **32 KB increments**.[54] | Broadcasting one draw to many players produces many billable deliveries. |
| **WebSocket lifecycle** | Maximum connection duration **2 hours**; idle timeout **10 minutes**.[59] | Reconnect, resubscribe, and restore a snapshot during longer events. |
| **Amplify Hosting** | Pricing page lists 12-month allowances of **1,000 build minutes/month**, **5 GB CDN storage**, and **15 GB transfer/month**; backend resources are separately priced.[55] | Convenient, but not a permanent all-inclusive free stack. See eligibility caveat below. |
| **EC2 / Lightsail** | New-account EC2 benefits use credits; the earlier EC2 12-month rules depend on account creation date. The current catalog lists Lightsail as a **90-day trial on a Paid plan**.[56][50] | Neither is an established ongoing free conventional server for this new project. |

**Documentation ambiguity:** API Gateway and Amplify pricing pages retain 12-month free-tier language, while the current AWS Free Tier catalogs describe these services as credit-funded. Treat those allowances as **unconfirmed for a new account** until the actual account's offer is checked. Do not label them “always free” or use them to promise a $0 ongoing bill.[54][55][50]

**Pros**

- Recurring compute/database allowances can materially reduce a small application's bill.[51][52]
- CloudFront has a distinct ongoing $0 delivery plan, rather than only a startup trial.[49][50]
- Assessment: clear separation of frontend delivery, API execution, durable state, and push transport offers flexibility as the app grows.

**Cons**

- Account-plan expiry and separately metered services make AWS's “free” proposition more complex than a hard-cap frontend plan.[47][54][58]
- Budgets are **not a guaranteed spending cap**; AWS warns that costs can exceed notification thresholds before alerts arrive and continue increasing afterward.[57]
- Assessment: IAM, deployment, monitoring, database capacity mode, and connection recovery add operational complexity for a small project.

**Brews Bingo fit:** A strong candidate if low cost with monitored billing is acceptable. Not the first choice if the requirement is “no billing exposure under any circumstances.”

### 4.4 Netlify — viable frontend, watch the shared credit budget

**Overview.** Netlify includes deployment/CDN hosting, serverless functions, and managed storage/database offerings. Its Free plan is **300 credits/month with a hard limit and no auto recharge**.[11]

**Selected limits and terms**

- Current rates include **15 credits per production deployment**, **20 credits/GB bandwidth**, **2 credits per 10,000 web requests**, and **10 credits/GB-hour compute**. Several activities share the same monthly allowance.[9][11]
- Reaching the limit pauses sites until the next cycle or upgrade; one project's overuse can pause other projects on the account.[9]
- Netlify explicitly states that commercial projects can use its Free plan. The older announcement is cited only for this eligibility statement; its historic resource quotas are **not** used here.[10]

**Pros**

- Explicit commercial-project eligibility and a free plan without usage overage billing.[10][11]
- Integrated deployment, previews, custom-domain SSL, Functions, and CDN delivery.[11]

**Cons**

- Deployments consume the same credits as visitors and compute; frequent production releases can reduce event headroom.[9][11]
- A hard cap can take the website offline rather than merely disabling an optional feature.[9]
- Netlify's published WebSocket example delegates live connections to Ably. This research did not verify a current built-in persistent WebSocket-server offering; pair with a separately evaluated realtime/backend service rather than assume one.[43]

**Brews Bingo fit:** Good frontend alternative to Cloudflare. Netlify plus Supabase is worth evaluating, but both providers' quotas must be considered and no architecture is selected.

### 4.5 Render — convenient prototype backend, unsuitable free production default

**Overview.** Render offers free static sites and conventional web-service instances supporting HTTP and WebSocket traffic. Its own documentation states: **“Do not use them for production applications.”**[13]

**Selected limits**

- **750 free instance-hours per workspace per calendar month**, shared by free web services.[13]
- Services spin down after **15 minutes without inbound traffic**; waking typically takes **about one minute**.[13]
- Filesystem changes are lost on redeploy, restart, or spin-down. A local SQLite file is not durable game storage on this tier.[13]
- Free Postgres has **1 GB storage and expires after 30 days**; it is not a permanent free database.[13]
- Bandwidth/build overuse can incur charges when a payment method is present, or suspension/build restrictions otherwise. Unusually high service-initiated traffic, including external-database access, can cause suspension.[13]

**Pros**

- Conventional server deployment and WebSocket support are useful for validating a portable backend implementation.[13]
- Free static hosting and backend compute make early integration experiments accessible.[13]

**Cons**

- Idle wake-up latency is a poor fit for a host expecting an immediate response.[13]
- Ephemeral storage and expiring free Postgres conflict with durable-history expectations unless separate storage is added.[13]
- The provider's production warning weighs against making this the free live-event default.[13]

**Brews Bingo fit:** Development/prototype only on Free; reassess paid compute before production.

### 4.6 Supabase — managed backend and database, not the web frontend

**Overview.** Supabase supplies managed Postgres, APIs, Edge Functions, and Realtime. Evaluate it with a separate frontend host; do not assume a static web-app deployment service is part of this backend selection.[17]
Vercel is one such frontend/backend host: the Supabase Marketplace integration offers plans starting at $0, integrated project creation, and unified billing. See Section 4.2 for the corrected Vercel storage comparison.[67][68]

**Selected Free allowances**

- **500 MB database**, **1 GB file storage**, **5 GB egress plus 5 GB cached egress**, and **two active projects**.[17]
- Realtime: **200 concurrent connections**, **2 million messages/month**, and **100 messages/second** on Free.[17][20]
- Edge Functions: **500,000 invocations included**. Free projects pause after **one week of inactivity**.[17]
- Free database size above **500 MB** triggers read-only mode; automatic backups and point-in-time recovery are not included in Free. Plan exports/restores and monitor organization-level fair-use limits as well as per-project database size.[72][17]

**Pros**

- Database, server-side functions, and live updates can cover several backend needs without separately assembling each service.[17]
- Assessment: SQL is a reasonable candidate for cards, claims, and historical records; backend validation must still be designed, not delegated to untrusted client writes.

**Cons**

- A weekly or less frequent venue event can encounter inactivity pause; availability needs an explicit pre-event check.[17]
- Connections are not the same as people: host displays, multiple tabs, and reconnects count too. Throughput limits can disconnect clients or refuse joins, even below the monthly message allowance.[20]
- A managed backend does not automatically satisfy a requirement for a conventional custom server process.
- Commercial eligibility and project-specific terms still require review; no blanket legal approval is inferred from the pricing page.

**Brews Bingo fit:** Strong candidate if managed backend services are acceptable and measured concurrency/broadcast bursts fit the plan. Do not rely solely on the apparently generous monthly message allowance.

### 4.7 Firebase and Google Cloud Run — useful capabilities, billing-enabled backend

**Overview.** Distinguish **Firebase Hosting** for web assets from **Firebase App Hosting**, and distinguish the no-payment-method **Spark** plan from billing-enabled **Blaze**. Custom Functions are unavailable on Spark; Blaze unlocks Functions and services such as Cloud Run.[21][23]

**Selected allowances**

- Firebase Hosting: **10 GB storage and 360 MB/day transfer** at no cost.[21]
- Firestore Standard: **1 GiB storage, 50,000 document reads/day, 20,000 writes/day, and 10 GiB monthly egress** at no cost.[21]
- Realtime Database on Spark: **100 simultaneous connections**. This is a different product from Firestore.[21]
- Firebase Functions on Blaze list **2 million no-cost invocations/month**, with execution, networking, build, and artifact-storage limits also relevant.[21]
- Cloud Run's request-based free allowance, based on `us-central1` pricing, is **2 million requests, 180,000 vCPU-seconds, and 360,000 GiB-seconds/month**. Instance-based billing has different allowances; free usage is aggregated by billing account and excess usage is billable.[38]
- Cloud Run supports WebSockets, but open connections keep instances active and billed; requests have a maximum **60-minute timeout**, requiring reconnect logic.[45]

**Pros**

- Cloud Run can host a conventional containerized backend, while Firebase supplies frontend hosting and data services.[38][45][21]
- Firestore client persistence supports cached reads/writes and later synchronization on Android, Apple, and web platforms.[39]

**Cons**

- Hosting/frontend free limits do not make custom backend compute available on Spark; billing-enabled services expose the project to usage charges.[23][38]
- Long-lived connections change compute consumption and require reconnection; an idle-looking game can still have active server cost.[45]
- Firestore offline caching is not offline multi-device game coordination. Its last-write-wins handling of competing document changes must not be mistaken for correct draw arbitration.[39]

**Brews Bingo fit:** Consider Firebase Hosting plus Cloud Run and a chosen datastore if a conventional backend is desired and monitored pay-as-you-go exposure is acceptable. Do not count it as a strict no-billing solution.

### 4.8 Oracle Cloud Always Free — conventional server with operational caveats

**Overview.** Oracle provides ongoing Always Free compute/storage resources rather than only a trial. The current documentation lists up to two AMD micro VMs, plus an Ampere A1 allowance of **1,500 OCPU-hours and 9,000 GB-hours/month**, described as **2 OCPUs and 12 GB memory** for Always Free tenancies.[29][30]

Other listed allowances include **200 GB combined boot/block storage** and **10 TB/month outbound transfer**. Always Free compute must be provisioned in the home region, and insufficient host capacity can prevent allocation.[29]

**Pros**

- Actual VM resources allow a conventional API/WebSocket server and self-managed datastore, subject to resource limits.[29]
- Assessment: a portable server/container design could also be run on venue hardware later if the offline architecture calls for it.

**Cons**

- Oracle may reclaim idle Always Free instances under documented seven-day utilization criteria.[29]
- Free capacity may be unavailable when needed; a resource allowance is not a capacity reservation.[29]
- Assessment: OS patching, firewalling, TLS, database backups, recovery, and monitoring become our responsibility.
- Signup involves payment-card verification; do not treat “Always Free” as meaning no account prerequisites.[30]

**Brews Bingo fit:** Worth investigating if a conventional server and strict recurring infrastructure cost are priorities, but availability/reclamation risks make it a weak unattended live-event default. Use the current limits above, not older articles advertising a larger Ampere allowance.

### 4.9 Railway and Fly.io — supplementary findings

| Provider | Overview | Pros | Cons / fit |
| --- | --- | --- | --- |
| **Railway** | New-user trial provides $5 for up to 30 days, then a Free plan with **$1 recurring monthly credit**, with no rollover.[32] | Some ongoing allowance exists beyond the trial. | Do not assume $1 covers an event backend, database, and idle usage; assess actual metering before relying on it. Lower-priority candidate for this free-hosting goal. |
| **Fly.io** | Current trial ends after **2 total VM hours or 7 days**, whichever comes first; apps stop until a payment method is added after exhaustion.[37] | Can support a short deployment experiment. | Not an ongoing free plan for a recurring bingo night; exclude from the primary $0 shortlist. |

## 5. Cost and capacity illustration — not a forecast

**Assumptions only:** 100 player clients, one host, one audience display; four games in a three-hour event; at most 75 draws per game; one `5 × 5` card per player per game. These are not approved attendance or load requirements.

| Quantity | Calculation | Result |
| --- | --- | --- |
| Connected clients | 100 + 1 + 1 | 102 |
| Draws across event | 4 × 75 | 300 |
| Draw deliveries if each client receives every draw | 102 × 300 | 30,600 |
| Connection-minutes | 102 × 180 | 18,360 |
| Card cells across event | 100 × 4 × 5 × 5 | 10,000 |
| Requests with every client polling every five seconds | 102 × 180 × 60 ÷ 5 | 220,320 |

All arithmetic was calculated programmatically. This excludes initial page loads, joins, claims, marking, retries, heartbeats, recovery snapshots, security checks, logging, database operations, and other users of the same account.

**Implications:**

- Five-second polling in this illustration exceeds Workers Free's 100,000 daily dynamic-request allowance on its own. Live updates may reduce polling overhead, but their own request/duration limits still apply.[5][8]
- A broadcast can create a brief message burst; Supabase's 100-messages/second limit merits testing even though 102 connections are below its 200-connection cap.[20]
- Firebase Realtime Database Spark's 100-connection limit would not cover 102 simultaneous connections to that database. This does not establish a corresponding limit for Firestore.[21]
- AWS realtime charges depend on messages delivered and connection-minutes, not just the 300 host draw actions.[54]
- An illustrative Netlify month with four production deployments, 2 GB delivery, and 100,000 web requests consumes **120 credits**: `(4 × 15) + (2 × 20) + (100,000 ÷ 10,000 × 2)`, before compute and other metered features. This illustrates the shared budget, not a predicted bill.[9][11]

## 6. Offline operation and live-event reliability

**Engineering assessment:** No researched cloud offering alone guarantees continued synchronized gameplay after the venue loses all routes to that cloud.

The HLD should decide:

1. **Which components remain available:** cached web app, local draw history, cards, and any locally executable validation.
2. **Who owns draw authority while disconnected:** avoiding competing accepted draw sequences is more important than merely queueing requests.
3. **How devices communicate during a WAN outage:** a reachable venue-local backend/network, another approved local path, or a clearly defined fallback requiring business review.
4. **What happens to winning claims:** immediate local validation versus pending validation must be decided explicitly, not changed because a free host is inconvenient.
5. **How reconnection works:** ordered updates, idempotent commands, reconciliation, reconnect storms, and full-state recovery.
6. **What the host sees:** stale/disconnected status, unavailable claims, recovery progress, and quota-related failures must not masquerade as normal gameplay.

Firestore's cache and sync can help individual-device continuity, but its documented last-write-wins behavior does not solve these game-specific consistency decisions.[39]

Operational checks should include a pre-event restore/health test, enough remaining quota, backup/export testing, and simulated internet loss. Do not use artificial traffic or repeated trial accounts to evade provider idle rules or free-tier restrictions.

## 7. Shortlist and recommendations for HLD review

| Priority | Candidate | Why evaluate it | Decision gate |
| --- | --- | --- | --- |
| **1** | Cloudflare static frontend + Workers + SQLite-backed Durable Objects; consider D1 only if needed | Recurring free frontend/backend components and stateful coordination.[35][5][8] WebSocket support.[36] | Accept serverless model; verify commercial terms, CPU/duration headroom, and offline architecture. |
| **2** | Cloudflare or Netlify frontend + Supabase backend | Managed SQL, server-side functions, and Realtime; separates frontend from backend selection.[17][20] | Accept managed-backend model; validate message bursts, connections, pauses, and backups. |
| **3** | AWS CloudFront/S3 + Lambda + provisioned DynamoDB + chosen realtime transport | Recurring allowances for core components.[50][51][52] Separately metered transport.[54] | Accept Paid account/billing exposure; confirm account-specific offers and full-stack cost. |
| **4** | Static frontend + Google Cloud Run + selected durable datastore | Conventional container backend and WebSockets.[38][45] | Accept billing-enabled account and websocket runtime costs; design external durable state. |
| **Shortlisted — eligibility confirmed by user** | Vercel Hobby frontend + Rust backend Functions + Neon Free or Supabase Free | Free hosting/Functions plus concrete Marketplace Postgres options.[1][63][67] | Validate Rust runtime/driver integration, database quotas, sleep/pauses, backups, realtime transport, and offline behavior; do not treat persistence as a missing or necessarily paid component. |
| **Conditional** | Oracle Always Free VM | Ongoing conventional-server resources.[29] | Confirm capacity and tolerance for reclamation and self-management. |
| **Prototype / paid alternatives** | Render Free; Railway; Fly.io; Vercel Pro if needed | Prototype restrictions and limited credits remain relevant.[13][32][37] Pro is a paid upgrade option.[40] | Do not promote prototype hosting to production without a fresh review; consider Pro if Hobby's usage allowances or features do not fit. |

**Recommendation:** Compare Cloudflare's integrated stateful backend against **Vercel Hobby + Neon Free** and **Vercel Hobby + Supabase Free** on technical suitability, full-stack quotas, and operational reliability. Hobby eligibility is resolved, and free persistent Postgres is available; remaining decisions concern capacity, database/realtime integration, and recovery, not whether free storage exists. Also consider the managed-backend alternatives above. If “backend server” specifically means a conventional always-running process, shift that comparison to Cloud Run versus Oracle, with Render paid hosting as a possible later low-cost option. Do not silently reinterpret the requirement to make a free tier fit.

## 8. Open choices before selecting a host

- Does “free” mean no recurring subscription, or **no possibility of charges**? Is a billing-enabled account acceptable?
- Does a serverless/managed backend satisfy the server requirement?
- What are expected and maximum players, tabs/devices, simultaneous games, event frequency, and retention?
- For a Vercel design, should persistence use Neon Free or Supabase Free, and should live updates use Vercel WebSockets with explicit cross-instance coordination or Supabase Realtime? Check pool sizing, remaining database compute/egress, and backup/restore requirements.
- Which backend capabilities must keep working during internet loss, and is venue-local infrastructure acceptable?
- Is a static frontend sufficient, or will server-side rendering be needed?
- Which region best serves the venue and any data-location requirements?
- Who owns billing accounts, deploy permissions, backups, and incident response?
- Are inactivity pauses and free-plan interruptions acceptable, or is a small production budget warranted?
- What are the data-export and migration paths if a provider changes its free offer?

## 9. Research scope and limitations

- Official provider pricing, product documentation, and provider-authored announcements were used for the cited findings. Search-only snippets and community posts were not used as authority in this report.
- No accounts were created, no card information was supplied, no deployments were performed, and no performance/SLA claims were tested.
- Source retrieval rate limits were handled with later retrieval or direct browser checks; no blocked-page contents were invented.
- The principal unresolved pricing ambiguity is AWS's newer credit-based model alongside older-looking Amplify/API Gateway allowance tables.[50][53][54]
- This is not a legal review of hosting terms or bingo/prize rules. Vercel Hobby eligibility is recorded as a user-confirmed project input in Section 4.2, not an open research question. Unresolved eligibility findings for other providers are unchanged.
- Domain registration, native app distribution, paid add-ons, and other non-hosting expenses are outside the $0 hosting comparison.
- This research does not modify approved business requirements, choose the HLD architecture, or create the LLD.

## Sources

[1] https://vercel.com/docs/plans/hobby — Vercel Hobby Plan
[2] https://vercel.com/docs/limits/fair-use-guidelines — Fair Use Guidelines - Vercel
[5] https://developers.cloudflare.com/workers/platform/pricing — Pricing · Cloudflare Workers docs
[8] https://developers.cloudflare.com/durable-objects/platform/pricing — Pricing · Cloudflare Durable Objects docs
[9] https://www.netlify.com/pricing — Pricing and Plans | Netlify
[10] https://www.netlify.com/blog/introducing-netlify-free-plan — Introducing Netlify's Free plan
[11] https://docs.netlify.com/manage/accounts-and-billing/billing/billing-for-credit-based-plans/credit-based-pricing-plans — Credit-based pricing plans | Netlify Docs
[13] https://render.com/docs/free — Deploy for Free – Render Docs
[17] https://supabase.com/pricing — Pricing & Fees
[20] https://supabase.com/docs/guides/realtime/limits — Realtime Limits - Docs
[21] https://firebase.google.com/pricing — Firebase Pricing - Google
[23] https://firebase.google.com/docs/projects/billing/firebase-pricing-plans — Firebase pricing plans - Google
[25] https://vercel.com/docs/functions/websockets — WebSockets - Vercel
[27] https://developers.cloudflare.com/pages/platform/limits — Limits · Cloudflare Pages docs
[29] https://docs.oracle.com/iaas/Content/FreeTier/freetier_topic-Always_Free_Resources.htm — Always Free Resources - Oracle Help Center
[30] https://www.oracle.com/cloud/free — Oracle Cloud Free Tier
[32] https://docs.railway.com/pricing/free-trial — Free Trial | Railway Docs
[35] https://developers.cloudflare.com/workers/static-assets/billing-and-limitations — Billing and Limitations · Cloudflare Workers docs
[36] https://developers.cloudflare.com/durable-objects/best-practices/websockets — Use WebSockets · Cloudflare Durable Objects docs
[37] https://docs.fly.io/about/free-trial — Fly.io Free Trial - Fly.io
[38] https://cloud.google.com/run/pricing — Cloud Run pricing | Google Cloud
[39] https://firebase.google.com/docs/firestore/manage-data/enable-offline — https://firebase.google.com/docs/firestore/manage-data/enable-offline
[40] https://vercel.com/pricing — https://vercel.com/pricing
[43] https://www.netlify.com/blog/web-sockets-in-a-serverless-world — Build Real-Time Applications with Web Sockets + Serverless - Netlify
[45] https://docs.cloud.google.com/run/docs/triggering/websockets — Using WebSockets | Cloud Run - Google Cloud Documentation
[47] https://aws.amazon.com/free/free-tier-faqs — AWS Free Tier FAQ
[48] https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/flat-rate-pricing-plan.html — CloudFront flat-rate pricing plans
[49] https://aws.amazon.com/cloudfront/pricing — CloudFront pricing
[50] https://aws.amazon.com/free/webapps — AWS free web applications catalog
[51] https://aws.amazon.com/lambda/pricing — Lambda pricing
[52] https://aws.amazon.com/dynamodb/pricing — DynamoDB pricing
[53] https://aws.amazon.com/free/serverless — AWS free serverless catalog
[54] https://aws.amazon.com/api-gateway/pricing — API Gateway pricing
[55] https://aws.amazon.com/amplify/pricing — Amplify pricing
[56] https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ec2-free-tier-usage.html — EC2 Free Tier eligibility
[57] https://docs.aws.amazon.com/cost-management/latest/userguide/budgets-managing-costs.html — AWS Budgets limitations
[58] https://aws.amazon.com/s3/pricing — S3 pricing
[59] https://docs.aws.amazon.com/apigateway/latest/developerguide/apigateway-execution-service-websocket-limits-table.html — API Gateway WebSocket limits
[60] https://docs.aws.amazon.com/awsaccountbilling/latest/aboutv2/free-tier-plans.html — AWS account plans
[61] https://vercel.com/docs/postgres — Postgres on Vercel
[62] https://vercel.com/docs/marketplace-storage — Storage on Vercel Marketplace
[63] https://vercel.com/marketplace/neon — Neon for Vercel
[65] https://neon.com/docs/introduction/plans — Neon plans - Neon Docs
[67] https://vercel.com/marketplace/supabase — Supabase for Vercel
[68] https://supabase.com/docs/guides/integrations/vercel-marketplace — Vercel Marketplace | Supabase Docs
[69] https://neon.com/docs/connect/connection-pooling — Connection pooling - Neon Docs
[70] https://neon.com/faqs/free-plan-limits-and-quotas — What are the limits and quotas for Neon's Free plan? - Neon FAQs
[71] https://neon.com/docs/guides/rust — Connect a Rust application to Lakebase Postgres - Neon Docs
[72] https://supabase.com/docs/guides/platform/database-size — Understanding Database and Disk Size | Supabase Docs
