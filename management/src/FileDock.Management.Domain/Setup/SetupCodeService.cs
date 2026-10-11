using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using FileDock.Management.Domain.Stores;

namespace FileDock.Management.Domain.Setup;

public sealed class SetupThrottleException : Exception
{
    public SetupThrottleException() : base("Too many setup-code attempts. Try again in 15 minutes.") { }
}

public sealed class SetupCodeService(ISetupStore store)
{
    public const string Awaiting = "AwaitingSetup";
    public const string Complete = "Complete";
    private const string Alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    public async Task<bool> IsCompleteAsync(CancellationToken cancellationToken = default)
    {
        var state = await store.GetAsync(cancellationToken);
        return state.Status == Complete;
    }

    public async Task<string> IssueAsync(CancellationToken cancellationToken = default)
    {
        var state = await store.GetAsync(cancellationToken);
        if (state.Status == Complete)
        {
            throw new InvalidOperationException("Setup is already complete.");
        }

        var raw = new char[8];
        var bytes = RandomNumberGenerator.GetBytes(8);
        for (var i = 0; i < raw.Length; i++)
        {
            raw[i] = Alphabet[bytes[i] % Alphabet.Length];
        }

        var code = new string(raw);
        state.SetupCodeHash = SHA256.HashData(Encoding.UTF8.GetBytes(code));
        state.SetupCodeIssuedAt = DateTimeOffset.UtcNow;
        state.AttemptCounters = "{}";
        state.Status = Awaiting;
        await store.SaveAsync(state, cancellationToken);
        return $"{code[..4]}-{code[4..]}";
    }

    public async Task VerifyAsync(string code, string clientIp, CancellationToken cancellationToken = default)
    {
        var state = await store.GetAsync(cancellationToken);
        if (state.Status == Complete)
        {
            throw new InvalidOperationException("Setup is already complete.");
        }

        var counters = AttemptLog.Parse(state.AttemptCounters);
        if (!counters.Allow(clientIp))
        {
            throw new SetupThrottleException();
        }

        counters.Record(clientIp);
        state.AttemptCounters = counters.ToJson();
        await store.SaveAsync(state, cancellationToken);

        var compact = code.Replace("-", "", StringComparison.Ordinal).ToUpperInvariant();
        var hash = SHA256.HashData(Encoding.UTF8.GetBytes(compact));
        if (state.SetupCodeHash is null || !CryptographicOperations.FixedTimeEquals(hash, state.SetupCodeHash))
        {
            throw new ArgumentException("The setup code is not valid.", nameof(code));
        }
    }

    public async Task MarkCompleteAsync(CancellationToken cancellationToken = default)
    {
        var state = await store.GetAsync(cancellationToken);
        state.Status = Complete;
        state.SetupCodeHash = null;
        await store.SaveAsync(state, cancellationToken);
    }
}

internal sealed class AttemptLog
{
    public List<DateTimeOffset> Total { get; set; } = [];
    public Dictionary<string, List<DateTimeOffset>> Ips { get; set; } = new(StringComparer.Ordinal);

    public static AttemptLog Parse(string json)
    {
        try
        {
            return JsonSerializer.Deserialize<AttemptLog>(json) ?? new AttemptLog();
        }
        catch (JsonException)
        {
            return new AttemptLog();
        }
    }

    public string ToJson() => JsonSerializer.Serialize(this);

    public bool Allow(string ip)
    {
        Prune();
        return Total.Count < 5 && (!Ips.TryGetValue(ip, out var list) || list.Count < 5);
    }

    public void Record(string ip)
    {
        Prune();
        var now = DateTimeOffset.UtcNow;
        Total.Add(now);
        if (!Ips.TryGetValue(ip, out var list))
        {
            list = [];
            Ips[ip] = list;
        }

        list.Add(now);
    }

    private void Prune()
    {
        var cutoff = DateTimeOffset.UtcNow.AddMinutes(-15);
        Total = Total.Where(t => t > cutoff).ToList();
        foreach (var key in Ips.Keys.ToList())
        {
            Ips[key] = Ips[key].Where(t => t > cutoff).ToList();
            if (Ips[key].Count == 0)
            {
                Ips.Remove(key);
            }
        }
    }
}
