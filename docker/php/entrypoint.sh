#!/bin/bash
set -e

echo "Starting Secret Santa application..."

# Wait for database to be ready
echo "Waiting for database connection..."
until php -r "
\$db = @new PDO('mysql:host=mariadb;port=3306', '${DB_USERNAME}', '${DB_PASSWORD}');
if (\$db) {
    echo 'Database connection successful';
    exit(0);
}
exit(1);
" 2>/dev/null; do
    echo "Database is unavailable - sleeping"
    sleep 2
done

echo "Database is ready!"

# Run database initialization
echo "Checking database schema..."
php /var/www/docker/php/init-database.php

echo "Starting PHP-FPM..."
php-fpm -D

echo "Starting Caddy..."
exec "$@"
