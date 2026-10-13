using System.Text.Json;
using FileDock.Management.Core;
using Microsoft.Extensions.Logging.Abstractions;
namespace FileDock.Management.UnitTests;

public sealed class EventRelayTests
{
    [Theory]
    [InlineData("1.0", true)]
    [InlineData("1.9", true)]
    [InlineData("2.0", false)]
    [InlineData("invalid", false)]
    public void InterfaceMajorMustMatch(string version, bool compatible) => Assert.Equal(compatible, CoreHealthMonitor.IsCompatible(version));
    [Fact]
    public async Task RelayPersistsAfterDeliveryAndResumesFromCursor()
    {
        var sink = new Sink(); var source = new Source();
        using var health = new CoreHealthMonitor(new LocalCoreGateway(), sink, NullLogger<CoreHealthMonitor>.Instance);
        using (var relay = new EventRelay(source, sink, health, NullLogger<EventRelay>.Instance))
        {
            await relay.StartAsync(CancellationToken.None);
            await WaitAsync(() => sink.Cursor == 2);
            await relay.StopAsync(CancellationToken.None);
        }
        Assert.Equal(1UL, source.After); Assert.Equal("publish,save", string.Join(',', sink.Order));
        using var restarted = new EventRelay(source, sink, health, NullLogger<EventRelay>.Instance);
        await restarted.StartAsync(CancellationToken.None);
        await WaitAsync(() => source.After == 2);
        await restarted.StopAsync(CancellationToken.None);
    }
    [Fact]
    public async Task FailedDeliveryDoesNotAdvanceDurableCursor()
    {
        var sink = new Sink { FailPublish = true }; var source = new Source();
        using var health = new CoreHealthMonitor(new LocalCoreGateway(), sink, NullLogger<CoreHealthMonitor>.Instance);
        using var relay = new EventRelay(source, sink, health, NullLogger<EventRelay>.Instance);
        await relay.StartAsync(CancellationToken.None); await WaitAsync(() => sink.Attempted); await relay.StopAsync(CancellationToken.None);
        Assert.Equal(1UL, sink.Cursor);
    }
    private static async Task WaitAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        while (!condition()) { await Task.Delay(10, timeout.Token); }
    }
    private sealed class Sink : IManagementEventSink
    {
        public ulong Cursor = 1; public bool FailPublish; public bool Attempted; public List<string> Order { get; } = [];
        public Task<ulong> ReadCursorAsync(CancellationToken ct) => Task.FromResult(Cursor);
        public Task PublishAsync(CoreEventDto change, CancellationToken ct) { Attempted = true; if (FailPublish) { throw new IOException("delivery failed"); } Order.Add("publish"); return Task.CompletedTask; }
        public Task SaveCursorAsync(ulong seq, CancellationToken ct) { Order.Add("save"); Cursor = seq; return Task.CompletedTask; }
        public Task PublishHealthAsync(CoreHealth health, CancellationToken ct) => Task.CompletedTask;
    }
    private sealed class Source : ICoreEventSource
    {
        public ulong After;
        public async IAsyncEnumerable<CoreEventDto> StreamEventsAsync(ulong afterSeq, Action connected, [System.Runtime.CompilerServices.EnumeratorCancellation] CancellationToken ct)
        {
            After = afterSeq; connected();
            if (afterSeq < 2) { using var doc = JsonDocument.Parse("{}"); yield return new(2, null, "resource.changed", null, null, doc.RootElement.Clone()); }
            await Task.Delay(Timeout.Infinite, ct);
        }
    }
}
