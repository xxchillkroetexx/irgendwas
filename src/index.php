<?php
$host = getenv('DB_HOST') ?: 'mariadb';
$port = getenv('DB_PORT') ?: '3306';
$db   = getenv('DB_DATABASE') ?: 'irgendwas_db';
$user = getenv('DB_USERNAME') ?: 'irgendjemand';
$pass = getenv('DB_PASSWORD') ?: 'irgendeinpasswort';

echo "<html><head><title>irgendwas</title>";
echo "<style>
    body { font-family: Arial, sans-serif; max-width: 800px; margin: 0 auto; padding: 20px; }
    h1, h2 { color: #336699; }
    .success { color: green; }
    .error { color: red; }
</style>";
echo "</head><body>";

echo "<h1>irgendwas - PHP Web Application</h1>";

try {
    $pdo = new PDO(
        "mysql:host=$host;port=$port;dbname=$db;charset=utf8mb4",
        $user,
        $pass,
        [PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION]
    );
    echo "<p class='success'>✅ Successfully connected to MariaDB!</p>";
    
    $version = $pdo->query('SELECT VERSION()')->fetchColumn();
    echo "<p>Database version: $version</p>";
    
} catch (PDOException $e) {
    echo "<p class='error'>❌ Database connection failed: " . $e->getMessage() . "</p>";
}

echo "<h2>System Information</h2>";
phpinfo();
echo "</body></html>";
?>
