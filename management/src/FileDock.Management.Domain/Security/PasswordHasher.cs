using System.Security.Cryptography;
using System.Text;
using Konscious.Security.Cryptography;

namespace FileDock.Management.Domain.Security;

public sealed class PasswordHasher
{
    public static void EnsurePolicy(string password)
    {
        if (string.IsNullOrEmpty(password))
        {
            throw new ArgumentException("A password is required.", nameof(password));
        }
    }

    public static string Hash(string password)
    {
        EnsurePolicy(password);
        var salt = RandomNumberGenerator.GetBytes(16);
        var hash = Derive(password, salt);
        return $"argon2id$v=19$m=65536,t=3,p=1${Convert.ToBase64String(salt)}${Convert.ToBase64String(hash)}";
    }

    public static bool Verify(string password, string encoded)
    {
        var parts = encoded.Split('$');
        if (parts.Length != 5)
        {
            return false;
        }

        var salt = Convert.FromBase64String(parts[3]);
        var expected = Convert.FromBase64String(parts[4]);
        var actual = Derive(password, salt);
        return CryptographicOperations.FixedTimeEquals(actual, expected);
    }

    private static byte[] Derive(string password, byte[] salt)
    {
        using var argon = new Argon2id(Encoding.UTF8.GetBytes(password))
        {
            Salt = salt,
            MemorySize = 65536,
            Iterations = 3,
            DegreeOfParallelism = 1,
        };
        return argon.GetBytes(32);
    }
}
