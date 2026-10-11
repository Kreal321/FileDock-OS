using FileDock.Management.Domain.Security;

namespace FileDock.Management.UnitTests;

public sealed class PasswordHasherTests
{
    [Fact]
    public void RejectsShortPasswords()
    {
        Assert.Throws<ArgumentException>(() => PasswordHasher.Hash("short"));
    }

    [Fact]
    public void RoundTrips()
    {
        var encoded = PasswordHasher.Hash("correct horse");
        Assert.True(PasswordHasher.Verify("correct horse", encoded));
        Assert.False(PasswordHasher.Verify("wrong password", encoded));
    }
}
