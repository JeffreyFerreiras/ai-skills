var e = new OrderPlaced(order.Id);
db.Orders.Add(order);
db.Add(OutboxMessage.Of(e));
await db.SaveChangesAsync();
