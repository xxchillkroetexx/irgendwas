# Secret Santa Web Application

A web application for organizing Secret Santa gift exchanges. This application allows users to create groups, invite participants, create wishlists, and automatically assign gift givers and receivers.

## Features

- User registration and authentication system
- Create and manage multiple Secret Santa groups
- Invite participants via email
- Create and edit wishlists with prioritization
- Automatic random gift assignment with exclusion rules
- Email notifications for invitations and draw results
- Multilingual support (English and German)

## Requirements

- PHP 8.2 or higher
- MariaDB/MySQL database
- SMTP server for sending emails
- Composer (for development)

Or just docker with docker compose.

## Development with Docker Compose

The easiest way to get started is to use the included Docker setup:

```bash
# Clone the repository
git clone https://git.scheissndreck.de/irgendwas/irgendwas.git
cd irgendwas

# Copy environment configuration and modify as needed
cp .env.template .env

# Start Docker Compose stack
docker compose up -d
```

After starting the Docker Compose stack, the application will be available at http://localhost or the URL configured in your .env file.

**Note:** The database will be automatically initialized on first startup. No manual setup required!

## Configuration

All configuration is handled through environment variables in the `.env` file.

Make sure to review and adjust these settings before deployment:

### Database Settings

- `DB_HOST` - Database host (default: localhost)
- `DB_PORT` - Database port (default: 3306)
- `DB_DATABASE` - Database name (default: irgendwas_db)
- `DB_USERNAME` - Database username (default: irgendjemand)
- `DB_PASSWORD` - Database password (default: irgendeinpasswort)

### Email Settings

- `MAIL_HOST` - SMTP server host (default: smtp.example.com)
- `MAIL_PORT` - SMTP server port (default: 587)
- `MAIL_USERNAME` - SMTP username
- `MAIL_PASSWORD` - SMTP password
- `MAIL_ENCRYPTION` - SMTP encryption (tls, ssl) (default: tls)
- `MAIL_FROM_ADDRESS` - Sender email address (default: noreply@example.com)
- `MAIL_FROM_NAME` - Sender name (default: Secret Santa)

### Application Settings

- `APP_URL` - Base URL of the application (default: https://localhost)
- `APP_DEBUG` - Enable debugging mode (true/false) (default: false)

## Database Initialization

The database is automatically initialized when you start the Docker containers for the first time. The system will:

1. Wait for the database to be ready
2. Check if tables exist
3. Create the schema if needed

If you need to manually reinitialize the database, you can still visit:

```
https://localhost/setup.php
```

Or rebuild the containers:

```bash
./tools/rebuild-container.sh
```

## Directory Structure

```
.
├── docker/             # Docker configuration files
├── public/             # Public-facing files
│   ├── index.php       # Application entry point
│   └── assets/         # Static assets (CSS, JS, images)
├── src/                # Application source code
│   ├── Config/         # Configuration files
│   ├── Core/           # Core application functionality
│   ├── Controllers/    # Request handlers
│   ├── Database/       # Database and ORM implementation
│   ├── Localization/   # Internationalization
│   ├── Models/         # Domain models/entities
│   ├── Repositories/   # Data access layer
│   ├── Services/       # Business logic
│   └── Views/          # Templates/views
├── .env.template       # Environment variables template
└── docker-compose.yml  # Docker Compose configuration
```

## Custom ORM Implementation

The application includes a custom object-relational mapping (ORM) implementation using the Data Mapper pattern. The ORM provides:

- Mapping between database tables and PHP objects
- Lazy loading and eager loading of relationships
- Transaction support
- Flexible query building

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
