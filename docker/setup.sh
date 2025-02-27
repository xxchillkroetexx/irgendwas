#!/bin/bash

echo "Setting up Secret Santa Application..."

# Navigate to the project root
cd ..

# Install dependencies using Composer
echo "Installing PHP dependencies..."
if [ -f "composer.phar" ]; then
    php composer.phar install
else
    composer install
fi

# Start docker containers
echo "Starting Docker containers..."
docker-compose up -d

# Wait for MariaDB to be ready
echo "Waiting for MariaDB to be ready..."
sleep 15

# Run database initialization
echo "Initializing database..."
docker exec secretsanta_web php /var/www/html/init.php

echo "Setup complete! Your application should be available at:"
echo "- Application: http://localhost:8000"
echo "- PHPMyAdmin:  http://localhost:8080"
