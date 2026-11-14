db.Orders.Add(order);
await db.SaveChangesAsync();
await bus.Publish(
    new OrderPlaced(order.Id));
