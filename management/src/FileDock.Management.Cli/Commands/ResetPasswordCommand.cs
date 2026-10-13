namespace FileDock.Management.Cli.Commands;

public static class ResetPasswordCommand
{
    public static async Task<int> RunAsync(string socketPath, string username, bool json)
    {
        if (Console.IsInputRedirected) { Console.Error.WriteLine("Use an interactive local console to enter the new password."); return 1; }
        Console.Error.WriteLine("We recommend more than 8 characters.");
        var password = ReadHidden("New password: ");
        var confirmation = ReadHidden("Re-enter password: ");
        if (string.IsNullOrEmpty(password) || password != confirmation) { Console.Error.WriteLine("Enter a non-empty password and matching confirmation."); return 1; }
        return await AdminSocketClient.PostAsync(socketPath, "/admin/reset-password", new { username, password }, json);
    }
    private static string ReadHidden(string prompt)
    {
        Console.Error.Write(prompt);
        var value = new System.Text.StringBuilder();
        while (true)
        {
            var key = Console.ReadKey(intercept: true);
            if (key.Key == ConsoleKey.Enter) { Console.Error.WriteLine(); return value.ToString(); }
            if (key.Key == ConsoleKey.Backspace)
            {
                if (value.Length > 0)
                {
                    value.Length--;
                }
            }
            else if (!char.IsControl(key.KeyChar))
            {
                value.Append(key.KeyChar);
            }
        }
    }
}
