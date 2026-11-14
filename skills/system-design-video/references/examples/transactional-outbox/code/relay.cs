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
