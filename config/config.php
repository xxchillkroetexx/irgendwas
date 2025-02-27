<?php
// Application configuration
define('APP_NAME', 'Secret Santa');
define('APP_URL', 'http://localhost:8000');
define('DEBUG', true);

// Database configuration - updated for Docker environment
define('DB_HOST', 'mariadb'); // Changed to container name
define('DB_NAME', 'secretsanta');
define('DB_USER', 'irgendjemand'); // Use the value from .env file
define('DB_PASS', 'irgendeinpasswort'); // Use the value from .env file
define('DB_CHARSET', 'utf8mb4');
define('DB_PORT', 3306);

// Email configuration
define('SMTP_HOST', 'smtp.example.com');
define('SMTP_PORT', 587);
define('SMTP_USER', 'notifications@example.com');
define('SMTP_PASS', 'your-smtp-password');
define('SMTP_FROM', 'noreply@example.com');
define('SMTP_FROM_NAME', 'Secret Santa App');

// Session configuration
define('SESSION_LIFETIME', 7200); // 2 hours
define('SESSION_SECURE', false); // Set to true in production
define('SESSION_HTTP_ONLY', true);
