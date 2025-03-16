<?php

namespace SecretSanta\Database;

use SecretSanta\Config\Database;

/**
 * Abstract DataMapper class for database operations
 * 
 * This class provides the base functionality for object-relational mapping,
 * implementing the Data Mapper pattern to separate database access logic
 * from domain objects.
 */
abstract class DataMapper
{
    /**
     * Database connection
     * 
     * @var \mysqli
     */
    protected \mysqli $db;
    
    /**
     * The table name for this mapper
     * 
     * @var string
     */
    protected string $table;
    
    /**
     * The entity class name this mapper works with
     * 
     * @var string
     */
    protected string $entityClass;
    
    /**
     * List of database columns for the entity
     * 
     * @var array
     */
    protected array $columns = [];
    
    /**
     * The primary key column name
     * 
     * @var string
     */
    protected string $primaryKey = 'id';

    /**
     * Constructor - initializes the database connection
     */
    public function __construct()
    {
        $this->db = Database::getInstance()->getConnection();
    }

    /**
     * Find an entity by its ID
     * 
     * @param int $id The entity ID
     * @return object|null The entity object or null if not found
     */
    public function find(int $id)
    {
        $query = "SELECT * FROM {$this->table} WHERE {$this->primaryKey} = ? LIMIT 1";
        $stmt = $this->db->prepare($query);
        $stmt->bind_param('i', $id);
        $stmt->execute();
        $result = $stmt->get_result();
        $data = $result->fetch_assoc();
        $stmt->close();

        if (!$data) {
            return null;
        }

        return $this->mapToEntity($data);
    }

    /**
     * Find entities matching the given criteria
     * 
     * @param array $criteria Associative array of conditions (column => value)
     * @param array $orderBy Associative array for sorting (column => direction)
     * @param int|null $limit Maximum number of results to return
     * @param int|null $offset Number of results to skip
     * @return array Array of entity objects
     */
    public function findBy(array $criteria, array $orderBy = [], ?int $limit = null, ?int $offset = null)
    {
        $query = "SELECT * FROM {$this->table}";
        $params = [];
        $types = '';

        if (!empty($criteria)) {
            $conditions = [];
            foreach (array_keys($criteria) as $key) {
                $conditions[] = "$key = ?";
                $params[] = $criteria[$key];

                // Add parameter type
                if (is_int($criteria[$key])) {
                    $types .= 'i';
                } elseif (is_float($criteria[$key])) {
                    $types .= 'd';
                } elseif (is_bool($criteria[$key])) {
                    $types .= 'i';
                    // Convert boolean to int
                    $params[count($params) - 1] = (int)$criteria[$key];
                } else {
                    $types .= 's';
                }
            }
            $query .= " WHERE " . implode(' AND ', $conditions);
        }

        if (!empty($orderBy)) {
            $orders = [];
            foreach ($orderBy as $column => $direction) {
                $orders[] = "$column $direction";
            }
            $query .= " ORDER BY " . implode(', ', $orders);
        }

        if ($limit !== null) {
            $query .= " LIMIT ?";
            $params[] = $limit;
            $types .= 'i';

            if ($offset !== null) {
                $query .= " OFFSET ?";
                $params[] = $offset;
                $types .= 'i';
            }
        }

        $stmt = $this->db->prepare($query);

        // Bind parameters if we have any
        if (!empty($params)) {
            // Using reference binding for mysqli
            $bindParams = array($types);
            foreach ($params as $key => $value) {
                $bindParams[] = &$params[$key];
            }
            call_user_func_array([$stmt, 'bind_param'], $bindParams);
        }

        $stmt->execute();
        $result = $stmt->get_result();

        $entities = [];
        while ($data = $result->fetch_assoc()) {
            $entities[] = $this->mapToEntity($data);
        }

        $stmt->close();
        return $entities;
    }

    /**
     * Find all entities in the table
     * 
     * @param array $orderBy Associative array for sorting (column => direction)
     * @param int|null $limit Maximum number of results to return
     * @param int|null $offset Number of results to skip
     * @return array Array of entity objects
     */
    public function findAll(array $orderBy = [], ?int $limit = null, ?int $offset = null)
    {
        return $this->findBy([], $orderBy, $limit, $offset);
    }

    /**
     * Save an entity (insert or update)
     * 
     * @param object $entity The entity to save
     * @return object The saved entity with updated data
     */
    public function save($entity)
    {
        $data = $this->mapFromEntity($entity);

        if (empty($data[$this->primaryKey])) {
            return $this->insert($data);
        } else {
            return $this->update($data);
        }
    }

    /**
     * Delete an entity from the database
     * 
     * @param object $entity The entity to delete
     * @return bool True if deletion was successful
     */
    public function delete($entity): bool
    {
        $data = $this->mapFromEntity($entity);

        if (empty($data[$this->primaryKey])) {
            return false;
        }

        $stmt = $this->db->prepare("DELETE FROM {$this->table} WHERE {$this->primaryKey} = ?");
        $id = $data[$this->primaryKey];
        $stmt->bind_param('i', $id);
        $stmt->execute();
        $affectedRows = $stmt->affected_rows;
        $stmt->close();

        return $affectedRows > 0;
    }

    /**
     * Delete an entity by its ID
     * 
     * @param int $id The ID of the entity to delete
     * @return bool True if deletion was successful
     */
    public function deleteById(int $id): bool
    {
        $stmt = $this->db->prepare("DELETE FROM {$this->table} WHERE {$this->primaryKey} = ?");
        $stmt->bind_param('i', $id);
        $stmt->execute();
        $affectedRows = $stmt->affected_rows;
        $stmt->close();

        return $affectedRows > 0;
    }

    /**
     * Insert a new entity into the database
     * 
     * @param array $data The entity data
     * @return object The created entity
     */
    protected function insert(array $data)
    {
        // Automatically set created_at and updated_at timestamps if they exist in the columns
        $currentDateTime = date('Y-m-d H:i:s');
        if (in_array('created_at', $this->columns) && !isset($data['created_at'])) {
            $data['created_at'] = $currentDateTime;
        }
        if (in_array('updated_at', $this->columns) && !isset($data['updated_at'])) {
            $data['updated_at'] = $currentDateTime;
        }

        $columns = array_keys(array_filter($data, function ($key) use ($data) {
            return $key !== $this->primaryKey || $data[$this->primaryKey] !== null;
        }, ARRAY_FILTER_USE_KEY));

        $placeholders = array_fill(0, count($columns), '?');

        $columnString = implode(', ', $columns);
        $placeholderString = implode(', ', $placeholders);

        $query = "INSERT INTO {$this->table} ($columnString) VALUES ($placeholderString)";
        $stmt = $this->db->prepare($query);

        // Build types string and values array
        $types = '';
        $values = [];

        foreach ($columns as $column) {
            $value = $data[$column];

            // Determine parameter type
            if (is_int($value)) {
                $types .= 'i';
            } elseif (is_float($value)) {
                $types .= 'd';
            } elseif (is_bool($value)) {
                $types .= 'i';
                $value = $value ? 1 : 0;
            } else {
                $types .= 's';
            }

            $values[] = $value;
        }

        // Using reference binding for mysqli
        $bindParams = array($types);
        foreach ($values as $key => $value) {
            $bindParams[] = &$values[$key];
        }
        call_user_func_array([$stmt, 'bind_param'], $bindParams);

        $stmt->execute();

        if (!isset($data[$this->primaryKey])) {
            $data[$this->primaryKey] = (int) $this->db->insert_id;
        }

        $stmt->close();

        return $this->mapToEntity($data);
    }

    /**
     * Update an existing entity in the database
     * 
     * @param array $data The entity data
     * @return object The updated entity
     */
    protected function update(array $data)
    {
        $id = $data[$this->primaryKey];

        // Automatically set updated_at timestamp if it exists in the columns
        if (in_array('updated_at', $this->columns) && !isset($data['updated_at'])) {
            $data['updated_at'] = date('Y-m-d H:i:s');
        }

        $sets = [];
        $columns = [];
        $values = [];
        $types = '';

        foreach ($data as $column => $value) {
            if ($column !== $this->primaryKey) {
                $sets[] = "$column = ?";
                $columns[] = $column;

                // Determine parameter type and adjust value if needed
                if (is_int($value)) {
                    $types .= 'i';
                    $values[] = $value;
                } elseif (is_float($value)) {
                    $types .= 'd';
                    $values[] = $value;
                } elseif (is_bool($value)) {
                    $types .= 'i';
                    $values[] = $value ? 1 : 0;
                } else {
                    $types .= 's';
                    $values[] = $value;
                }
            }
        }

        $setString = implode(', ', $sets);
        $query = "UPDATE {$this->table} SET $setString WHERE {$this->primaryKey} = ?";

        // Add the ID parameter type and value
        $types .= 'i';
        $values[] = $id;

        $stmt = $this->db->prepare($query);

        // Using reference binding for mysqli
        $bindParams = array($types);
        foreach ($values as $key => $value) {
            $bindParams[] = &$values[$key];
        }
        call_user_func_array([$stmt, 'bind_param'], $bindParams);

        $stmt->execute();
        $stmt->close();

        return $this->find($id);
    }

    /**
     * Start a database transaction
     */
    public function beginTransaction()
    {
        $this->db->begin_transaction();
    }

    /**
     * Commit the current database transaction
     */
    public function commit()
    {
        $this->db->commit();
    }

    /**
     * Roll back the current database transaction
     */
    public function rollback()
    {
        $this->db->rollback();
    }

    /**
     * Map database data to an entity object
     * 
     * @param array $data The database data
     * @return object The created entity
     */
    protected function mapToEntity(array $data)
    {
        return new $this->entityClass($data);
    }

    /**
     * Map an entity object to database data
     * 
     * @param object $entity The entity to map
     * @return array The database data
     */
    protected function mapFromEntity($entity): array
    {
        if (method_exists($entity, 'toArray')) {
            return $entity->toArray();
        }

        $data = [];
        foreach ($this->columns as $column) {
            $getter = 'get' . ucfirst($column);
            if (method_exists($entity, $getter)) {
                $data[$column] = $entity->$getter();
            }
        }

        return $data;
    }
}
