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

## Development with Docker Compose

The easiest way to get started is to use the included Docker setup:

```bash
# Clone the repository
git clone https://github.com/yourusername/secret-santa.git
cd secret-santa

# Copy environment configuration and modify as needed
cp .env.template .env

# Start Docker Compose stack
docker compose up -d
```

After starting the Docker Compose stack, the application will be available at http://localhost or the URL configured in your .env file.

## Database Initialization

The database schema will be automatically initialized on first run. If you need to reinitialize the database, uncomment the initialization line in `src/Core/Application.php`:

```php
// Uncomment to initialize database schema
$db->initialize();
```

## Configuration

All configuration is handled through environment variables in the `.env` file:

- Database connection details
- SMTP server settings for email notifications
- Application settings and security keys

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
│   ├── Database/       # Database and ORM implementation
│   ├── Models/         # Domain models/entities
│   ├── Repositories/   # Data access layer
│   ├── Controllers/    # Request handlers
│   ├── Services/       # Business logic
│   ├── Helpers/        # Utility functions
│   ├── Localization/   # Internationalization
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