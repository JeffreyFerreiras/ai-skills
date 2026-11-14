# Transactional outbox in EF Core: script v1 + storyboard (FOR REVIEW)

Status: SCRIPT IN PROGRESS (v1 for Jeffrey's review). Not narrated, not rendered.
Length: 251 words, 10 paragraphs, 29 sentences.
Projected narration: about 1:26 at Fenrir speed 1.20 (articulation 209 wpm measured on cartesian v4) with 0.42 s sentence and 0.68 s paragraph pauses, about 175 wpm overall.
Projected video: about 1:31 with the laptop close (1.55 s) and the Jeffrit end card (2.9 s). Hard cap 2:00.
Narration text only: canonical-narration.txt. Code cards: code/ (max 34 characters per line). Proof run: ../code-verification/.
Timestamps are estimates from word counts; they get retimed to the real narration.

## Spoken vs on screen
- Spoken: "Entity Framework Core" (first mention), then "Entity Framework"; "Save Changes"; "Order Placed". No spoken acronyms.
- Captions keep the canonical narration text. Code cards show `SaveChangesAsync`, `OrderPlaced`, `OutboxMessage` as code.
- "at-least-once" is a hyphenated word, not a dash. No em dashes anywhere.

## Script with storyboard

**0:00 to 0:05** (N01)
> The order saved. The confirmation email never came. And nothing in the logs says why.

Visual: detailed front-on chalk customer at a phone-sized checkout: green check "order saved". Beside it a chalk inbox stays empty; an envelope outline flickers and never arrives. A chalk developer front-on at a desk physically opens the laptop (lid rotates up by itself, no hands), log lines scroll with no warning. Labels: none.

**0:06 to 0:14** (N02)
> Here's the code. Save the order with Entity Framework Core, then publish Order Placed to the message broker, so email and inventory hear about it.

Visual: BAD card types in with syntax highlighting (code/bad.cs). As each line is spoken: an order card drops into a chalk database cylinder; an event envelope flies from the laptop to a chalk broker (a post-office style mailbox/queue), which fans out to two subscribers, an email node and an inventory shelf node.
```csharp
db.Orders.Add(order);
await db.SaveChangesAsync();
await bus.Publish(
    new OrderPlaced(order.Id));
```

**0:14 to 0:19** (N03)
> Two writes. One to the database, one to the broker. Nothing ties them together.

Visual: two separate chalk arrows from the laptop, one to the database, one to the broker; the gap between the two code lines widens on the card. A loose chalk string between the arrows snaps (nothing ties them).

**0:20 to 0:33** (N04)
> If the app crashes between them, or the broker is down, the order is committed, but the event is gone. Email never sends. Inventory never reserves. Publish first instead, and a rollback leaves everyone reacting to an order that doesn't exist.

Visual (causal core, two replays):
1. Order lands in the database and the database stamps a check. Then the laptop screen flashes and goes dark (crash) and the broker shows a red X (down); the envelope never leaves. Email node stays grey, inventory shelf keeps the item. 
2. Rewind: lines swap. Envelope goes first, subscribers react (email sends, shelf removes an item), then the database transaction rolls back: the order card slides back out and fades. The subscribers now point at an empty slot (ghost order).

**0:33 to 0:35** (N05)
> That's the dual write problem.

Visual: the two arrows from the laptop pulse together, then split apart. No text label (the narration names it).

**0:35 to 0:49** (N06)
> The fix is a transactional outbox. Don't publish yet. Add an outbox message, with the event inside, to the same context, and call Save Changes once. Entity Framework writes the order and the message in one transaction. Both commit, or neither does.

Visual: GOOD card (code/good-outbox.cs) types in; `db.Add(OutboxMessage.Of(e));` highlighted. Inside the database a second small table appears next to Orders: the outbox tray. The envelope is placed in the tray. One chalk transaction band wraps both the order card and the envelope, and they drop in together with one check. Then a forced failure replay: the band rolls both back together, neither row remains.
```csharp
var e = new OrderPlaced(order.Id);
db.Orders.Add(order);
db.Add(OutboxMessage.Of(e));
await db.SaveChangesAsync();
```

**0:50 to 1:00** (N07)
> Then a background service, the relay, polls the outbox table, publishes each unsent message, and marks it sent. Broker down? The message waits in the table, and the relay tries again.

Visual: RELAY card (code/relay.cs) types in, compact. A small chalk relay worker (a gear with a clock hand) beside the database ticks once per poll; it lifts unsent envelopes from the tray, carries them to the broker, then stamps each envelope sent (check). Broker shows a red X: the envelope stays in the tray (unsent), the relay ticks again, broker recovers, envelope goes, email and inventory light up.
```csharp
while (true)
{
    using var db = NewDb();
    var batch = await db.Outbox
        .Where(m => !m.Sent)
        .OrderBy(m => m.Id)
        .Take(50).ToListAsync(ct);
    foreach (var m in batch)
    {
        if (!await TrySend(m, ct))
            break;
        m.Sent = true;
    }
    await db.SaveChangesAsync(ct);
    await Task.Delay(1000, ct);
}
```

**1:00 to 1:11** (N08)
> Now the trade-offs. If the relay crashes after publishing but before marking it sent, it publishes again. That's at-least-once delivery, so consumers must be idempotent. Record every message you handle, and skip repeats.

Visual: relay publishes an envelope, then the relay gear cracks before the stamp; restart; the same envelope goes out again. At the email node two identical envelopes arrive; a chalk ledger beside the consumer records the first, the second matches a ledger row and is dropped (soft "nope"). Labels: none (a duplicate is shown as two identical envelopes).

**1:11 to 1:18** (N09)
> Delivery isn't instant, either. Polling adds a little delay. And the table keeps growing, so clean out sent rows.

Visual: a chalk clock between the tray and the broker shows a short wait per tick. The outbox tray fills with stamped envelopes; a chalk broom sweeps the stamped ones out, unsent ones stay.

**1:19 to 1:26** (N10)
> Rule of thumb: when a database change has to tell other services, write the news in the same transaction, and let the relay do the talking.

Visual: recap: order and envelope drop in together with one band, relay carries the envelope out, subscribers light up with checks. The developer front-on relaxes.

**1:26 to 1:31**
Visual: the laptop lid closes by itself (no hands), crossfade to the Jeffrit end card (assets/brand/jeffrit/endcard.py) with the music swell and resolving chord.

## Style and rules for the render
- Chalk board, physical laptop plate (lid opens and closes by itself), detailed front-on chalk people (developer, customer), no hands or arms.
- Minimal labels: numbers and code only; captions burned in from the canonical narration; no scene IDs or track labels on any output; no em dashes.
- Animated causal data flow: every arrival happens after its send; a check only after the commit; email or inventory reacts only after the envelope arrives; a duplicate is dropped only after the ledger has the first one.
- Syntax-highlighted C# cards typing in, readable on a phone (max 34 characters per line).
- Music and SFX as in cartesian explosion final-v2: high-passed, no rumble or buzz, -14 LUFS, true peak at most -2 dBTP.

## Sources (Microsoft Learn)
1. Azure Architecture Center, "Implement the Transactional Outbox pattern by using Azure Cosmos DB": https://learn.microsoft.com/en-us/azure/architecture/databases/guide/transactional-outbox-cosmos
   Used for: the ordering example (order saved, then OrderCreated publish fails on network error, message service outage or host failure, so Inventory and Invoicing are not notified); the fix (save the business object and its events in the same transaction, outbox table, worker queries unhandled entries, publishes, marks processed); "when a service uses Entity Framework Core, it creates a database transaction ... saves the business object and event"; duplicates on reprocessing and deduplication by MessageId; cleanup of old events; event order.
2. .NET microservices architecture, "Subscribing to events" (Designing atomicity and resiliency when publishing to the event bus; Idempotency in update message events; Deduplicating integration event messages): https://learn.microsoft.com/en-us/dotnet/architecture/microservices/multi-container-microservice-net-applications/subscribe-events
   Used for: the atomicity problem of updating the database and publishing an event; outbox as "a transactional table to store the integration events (extending the local transaction)"; retries can publish an event that was already published, so updates must be idempotent or detect and discard duplicates.
3. .NET microservices architecture, "Implementing event-based communication between microservices (integration events)": https://learn.microsoft.com/en-us/dotnet/architecture/microservices/multi-container-microservice-net-applications/integration-event-based-microservice-communications
   Used for: transactions do not span the persistence and the event bus, so idempotence needs to be handled.
4. EF Core, "Transactions": https://learn.microsoft.com/en-us/ef/core/saving/transactions
   Used for: all changes in a single SaveChanges call are applied in one transaction; it either completely succeeds or leaves the database unmodified.
5. .NET, "Worker services in .NET" (BackgroundService): https://learn.microsoft.com/en-us/dotnet/core/extensions/workers
   Used for: the relay as a hosted BackgroundService with ExecuteAsync.

## Code verification
../code-verification (net9.0, EF Core Sqlite 9.0.10, warnings as errors). gen.py pastes the three cards verbatim into method bodies; Program.cs runs the story:
bad path loses the event; outbox row and order commit together; a failing SaveChanges stores neither; the relay keeps polling while the broker is down and publishes once it is back; a crash after publishing and before marking sent republishes, and a consumer that records handled messages skips the duplicate. Output: ../code-verification/verify-output.txt.
Note: the relay card is the ExecuteAsync loop; `NewDb` (a fresh DbContext per poll) and `TrySend` (returns false while the broker is unavailable) live in the same BackgroundService class in the verification project.
