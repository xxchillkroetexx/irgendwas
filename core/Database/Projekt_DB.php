<?php
namespace core\Database;

class Projekt_DB {
    private static $instance = null;
    private $connection = null;
    private $lastInsertId = null;
    private $transactionCount = 0;
    
    // Private constructor for singleton pattern
    private function __construct() {
        $this->connect();
    }
    
    // Get singleton instance
    public static function getInstance() {
        if (self::$instance === null) {
            self::$instance = new self();
        }
        return self::$instance;
    }
    
    // Connect to the database
    private function connect() {
        try {
            // Include port number in DSN
            $dsn = 'mysql:host=' . DB_HOST . ';port=' . (defined('DB_PORT') ? DB_PORT : 3306) . ';dbname=' . DB_NAME . ';charset=' . DB_CHARSET;
            $options = [
                \PDO::ATTR_ERRMODE => \PDO::ERRMODE_EXCEPTION,
                \PDO::ATTR_DEFAULT_FETCH_MODE => \PDO::FETCH_ASSOC,
                \PDO::ATTR_EMULATE_PREPARES => false,
                \PDO::ATTR_PERSISTENT => true
            ];
            
            // Try to create connection
            $this->connection = new \PDO($dsn, DB_USER, DB_PASS, $options);
        } catch (\PDOException $e) {
            // Enhanced error message with more details
            $message = 'Database connection failed: ' . $e->getMessage();
            $message .= ' (Host: ' . DB_HOST . ', Database: ' . DB_NAME . ')';
            
            if (DEBUG) {
                echo '<div style="background: #f8d7da; color: #721c24; padding: 20px; margin: 20px; border-radius: 5px;">';
                echo '<h3>Database Connection Error</h3>';
                echo '<p>' . $message . '</p>';
                echo '<h4>Possible solutions:</h4>';
                echo '<ol>';
                echo '<li>Check if MySQL/MariaDB server is running</li>';
                echo '<li>Verify the database credentials in config.php</li>';
                echo '<li>Make sure the database "' . DB_NAME . '" exists</li>';
                echo '<li>Try using "127.0.0.1" instead of "localhost" or vice versa</li>';
                echo '</ol>';
                echo '</div>';
            }
            
            throw new \Exception($message);
        }
    }
    
    // Get the PDO connection
    public function getConnection() {
        return $this->connection;
    }
    
    // Execute a query with parameters
    public function execute($sql, $params = []) {
        try {
            $statement = $this->connection->prepare($sql);
            $statement->execute($params);
            $this->lastInsertId = $this->connection->lastInsertId();
            return $statement;
        } catch (\PDOException $e) {
            $this->handleError($e, $sql, $params);
            return false;
        }
    }
    
    // Query and fetch all results
    public function fetchAll($sql, $params = [], $fetchMode = null) {
        $statement = $this->execute($sql, $params);
        if ($statement && $fetchMode) {
            return $statement->fetchAll($fetchMode);
        } elseif ($statement) {
            return $statement->fetchAll();
        }
        return [];
    }
    
    // Query and fetch a single row
    public function fetch($sql, $params = [], $fetchMode = null) {
        $statement = $this->execute($sql, $params);
        if ($statement && $fetchMode) {
            return $statement->fetch($fetchMode);
        } elseif ($statement) {
            return $statement->fetch();
        }
        return false;
    }
    
    // Query and fetch a single column value
    public function fetchColumn($sql, $params = [], $column = 0) {
        $statement = $this->execute($sql, $params);
        if ($statement) {
            return $statement->fetchColumn($column);
        }
        return false;
    }
    
    // Get the last inserted ID
    public function lastInsertId() {
        return $this->lastInsertId;
    }
    
    // Start a transaction
    public function beginTransaction() {
        if ($this->transactionCount === 0) {
            $this->connection->beginTransaction();
        }
        $this->transactionCount++;
    }
    
    // Commit a transaction
    public function commit() {
        if ($this->transactionCount === 1) {
            $this->connection->commit();
        }
        $this->transactionCount = max(0, $this->transactionCount - 1);
    }
    
    // Roll back a transaction
    public function rollBack() {
        if ($this->transactionCount === 1) {
            $this->connection->rollBack();
        }
        $this->transactionCount = max(0, $this->transactionCount - 1);
    }
    
    // Create a query builder instance
    public function table($table) {
        return new DB_Query($this, $table);
    }
    
    // Handle database errors
    private function handleError(\PDOException $e, $sql, $params) {
        $message = 'Database query error: ' . $e->getMessage();
        $context = [
            'sql' => $sql,
            'params' => $params,
            'error' => $e->getMessage(),
            'trace' => $e->getTraceAsString()
        ];
        
        if (DEBUG) {
            echo '<h1>Database Error</h1>';
            echo '<p>' . $message . '</p>';
            echo '<h2>Query</h2>';
            echo '<pre>' . $sql . '</pre>';
            echo '<h2>Parameters</h2>';
            echo '<pre>' . print_r($params, true) . '</pre>';
        }
        
        // Log the error
        error_log($message . ' ' . json_encode($context));
    }
    
    // Create database tables if they don't exist
    public function createTables() {
        $queries = [
            // Users table
            "CREATE TABLE IF NOT EXISTS users (
                id INT AUTO_INCREMENT PRIMARY KEY,
                email VARCHAR(255) NOT NULL UNIQUE,
                password VARCHAR(255) NOT NULL,
                first_name VARCHAR(100) NOT NULL,
                last_name VARCHAR(100) NOT NULL,
                reset_token VARCHAR(100) NULL,
                reset_token_expiry DATETIME NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
            )",
            
            // Groups table
            "CREATE TABLE IF NOT EXISTS `groups` (
                id INT AUTO_INCREMENT PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                description TEXT NULL,
                admin_id INT NOT NULL,
                join_deadline DATETIME NULL,
                draw_date DATETIME NULL,
                is_drawn BOOLEAN DEFAULT FALSE,
                custom_email_template TEXT NULL,
                wishlist_visibility ENUM('all', 'santa_only') DEFAULT 'all',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (admin_id) REFERENCES users(id) ON DELETE CASCADE
            )",
            
            // Group members table
            "CREATE TABLE IF NOT EXISTS group_members (
                id INT AUTO_INCREMENT PRIMARY KEY,
                group_id INT NOT NULL,
                user_id INT NOT NULL,
                invitation_token VARCHAR(100) NULL,
                invitation_email VARCHAR(255) NULL,
                status ENUM('invited', 'active', 'declined') DEFAULT 'invited',
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (group_id) REFERENCES `groups`(id) ON DELETE CASCADE,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                UNIQUE KEY (group_id, user_id)
            )",
            
            // Wishlists table
            "CREATE TABLE IF NOT EXISTS wishlists (
                id INT AUTO_INCREMENT PRIMARY KEY,
                user_id INT NOT NULL,
                group_id INT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (group_id) REFERENCES `groups`(id) ON DELETE CASCADE,
                UNIQUE KEY (user_id, group_id)
            )",
            
            // Wishlist items table
            "CREATE TABLE IF NOT EXISTS wishlist_items (
                id INT AUTO_INCREMENT PRIMARY KEY,
                wishlist_id INT NOT NULL,
                name VARCHAR(255) NOT NULL,
                description TEXT NULL,
                link VARCHAR(512) NULL,
                sort_order INT DEFAULT 0,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (wishlist_id) REFERENCES wishlists(id) ON DELETE CASCADE
            )",
            
            // Assignments table
            "CREATE TABLE IF NOT EXISTS assignments (
                id INT AUTO_INCREMENT PRIMARY KEY,
                group_id INT NOT NULL,
                giver_id INT NOT NULL,
                receiver_id INT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (group_id) REFERENCES `groups`(id) ON DELETE CASCADE,
                FOREIGN KEY (giver_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (receiver_id) REFERENCES users(id) ON DELETE CASCADE,
                UNIQUE KEY (group_id, giver_id),
                UNIQUE KEY (group_id, receiver_id)
            )",
            
            // Restrictions table
            "CREATE TABLE IF NOT EXISTS restrictions (
                id INT AUTO_INCREMENT PRIMARY KEY,
                group_id INT NOT NULL,
                user_id INT NOT NULL,
                restricted_user_id INT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (group_id) REFERENCES `groups`(id) ON DELETE CASCADE,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE,
                FOREIGN KEY (restricted_user_id) REFERENCES users(id) ON DELETE CASCADE,
                UNIQUE KEY (group_id, user_id, restricted_user_id)
            )"
        ];
        
        foreach ($queries as $query) {
            $this->execute($query);
        }
    }
}
