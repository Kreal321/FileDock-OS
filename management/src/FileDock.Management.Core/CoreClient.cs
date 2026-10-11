using System.Net.Sockets;
using Filedock.Core.V1;
using Grpc.Core;
using Grpc.Net.Client;
using Microsoft.Extensions.Configuration;

namespace FileDock.Management.Core;

public sealed class CoreClient : IAsyncDisposable
{
    public const string InterfaceVersion = "1.0";

    private readonly GrpcChannel _channel;

    public CoreControl.CoreControlClient Control { get; }
    public Secrets.SecretsClient Secrets { get; }
    public Observability.ObservabilityClient Observability { get; }
    public Connectors.ConnectorsClient Connectors { get; }

    public CoreClient(IConfiguration configuration)
    {
        var socket = configuration["Core:Socket"] ?? "/run/filedock/core.sock";
        var sockets = new SocketsHttpHandler
        {
            ConnectCallback = async (_, cancellationToken) =>
            {
                var unix = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
                await unix.ConnectAsync(new UnixDomainSocketEndPoint(socket), cancellationToken);
                return new NetworkStream(unix, ownsSocket: true);
            },
        };
        _channel = GrpcChannel.ForAddress("http://localhost", new GrpcChannelOptions { HttpHandler = sockets });
        Control = new CoreControl.CoreControlClient(_channel);
        Secrets = new Secrets.SecretsClient(_channel);
        Observability = new Observability.ObservabilityClient(_channel);
        Connectors = new Connectors.ConnectorsClient(_channel);
    }

    public static Metadata Metadata(string actor, string requestId, bool mutating)
    {
        var meta = new Metadata { { "fd-interface-version", InterfaceVersion } };
        if (mutating)
        {
            meta.Add("fd-actor", actor);
            meta.Add("fd-request-id", requestId);
        }

        return meta;
    }

    public static (int Status, string Detail) ToProblem(RpcException error)
    {
        var status = error.StatusCode switch
        {
            StatusCode.InvalidArgument => 400,
            StatusCode.NotFound => 404,
            StatusCode.AlreadyExists => 409,
            StatusCode.Aborted => 409,
            StatusCode.FailedPrecondition => 412,
            StatusCode.PermissionDenied => 403,
            StatusCode.Unauthenticated => 401,
            StatusCode.ResourceExhausted => 507,
            _ => 502,
        };
        return (status, error.Status.Detail);
    }

    public ValueTask DisposeAsync()
    {
        _channel.Dispose();
        return ValueTask.CompletedTask;
    }
}
