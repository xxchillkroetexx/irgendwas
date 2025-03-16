<?php

namespace SecretSanta\Database;

use SecretSanta\Config\Database;

/**
 * Abstract DataMapper class for database operations
 * 
 * This class provides a base implementation for the Data Mapper design pattern,
 * handling basic CRUD operations and mapping between database and entity objects.
 */
abstract class DataMapper
{
    /**
     * Database connection instance
     * @var \PDO
     */
    protected \PDO $db;

    /**
     * Database table name
     * @var string
     */
    protected string $table;

    /**
     * Entity class name to map to/from
     * @var string
     */
    protected string $entityClass;

    /**
     * Database table columns
     * @var array
     */
    protected array $columns = [];

    /**
     * Primary key column name
     * @var string
     */
    protected string $primaryKey = 'id';

    /**
     * Constructor initializes the database connection
     */
    public function __construct()
    {
        $this->db = Database::getInstance()->getConnection();
    }

    /**
     * Find entity by primary key
     * 
     * @param int $id Primary key value
     * @return object|null Entity instance or null if not found
     */
    public function find(int $id)
    {
        $stmt = $this->db->prepare("SELECT * FROM {$this->table} WHERE {$this->primaryKey} = :id LIMIT 1");
        $stmt->execute(['id' => $id]);
        $data = $stmt->fetch();

        if (!$data) {
            return null;
        }

        return $this->mapToEntity($data);
    }

    /**
     * Find entities matching specified criteria
     * 
     * @param array $criteria Associative array of field-value pairs to filter by
     * @param array $orderBy Associative array of field-direction pairs for ordering
     * @param int|null $limit Maximum number of results to return
     * @param int|null $offset Number of results to skip
     * @return array Array of entity objects
     */
    public function findBy(array $criteria, array $orderBy = [], ?int $limit = null, ?int $offset = null)
    {
        $query = "SELECT * FROM {$this->table}";

        if (!empty($criteria)) {
            $conditions = [];
            foreach (array_keys($criteria) as $key) {
                $conditions[] = "$key = :$key";
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
            $query .= " LIMIT :limit";
            if ($offset !== null) {
                $query .= " OFFSET :offset";
            }
        }

        $stmt = $this->db->prepare($query);

        foreach ($criteria as $key => $value) {
            $stmt->bindValue(":$key", $value);
        }

        if ($limit !== null) {
            $stmt->bindValue(':limit', $limit, \PDO::PARAM_INT);
            if ($offset !== null) {
                $stmt->bindValue(':offset', $offset, \PDO::PARAM_INT);
            }
        }

        $stmt->execute();
        $results = $stmt->fetchAll();

        $entities = [];
        foreach ($results as $data) {
            $entities[] = $this->mapToEntity($data);
        }

        return $entities;
    }

    /**
     * Find all entities in the table
     * 
     * @param array $orderBy Associative array of field-direction pairs for ordering
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
     * @param object $entity Entity object to save
     * @return object Updated entity with any database-generated values
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
     * @param object $entity Entity object to delete
     * @return bool True if deletion was successful
     */
    public function delete($entity): bool
    {
        $data = $this->mapFromEntity($entity);

        if (empty($data[$this->primaryKey])) {
            return false;
        }

        $stmt = $this->db->prepare("DELETE FROM {$this->table} WHERE {$this->primaryKey} = :id");
        $stmt->execute(['id' => $data[$this->primaryKey]]);

        return $stmt->rowCount() > 0;
    }

    /**
     * Delete an entity by its primary key
     * 
     * @param int $id Primary key value
     * @return bool True if deletion was successful
     */
    public function deleteById(int $id): bool
    {
        $stmt = $this->db->prepare("DELETE FROM {$this->table} WHERE {$this->primaryKey} = :id");
        $stmt->execute(['id' => $id]);

        return $stmt->rowCount() > 0;
    }

    /**
     * Insert a new entity into the database
     * 
     * @param array $data Entity data as associative array
     * @return object Newly created entity with database-generated values
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

        $placeholders = array_map(function ($col) {
            return ":$col";
        }, $columns);

        $columnString = implode(', ', $columns);
        $placeholderString = implode(', ', $placeholders);

        $stmt = $this->db->prepare("INSERT INTO {$this->table} ($columnString) VALUES ($placeholderString)");

        foreach ($columns as $column) {
            // Convert boolean values to integers for MySQL
            if (is_bool($data[$column])) {
                $stmt->bindValue(":$column", $data[$column] ? 1 : 0, \PDO::PARAM_INT);
            } else {
                $stmt->bindValue(":$column", $data[$column]);
            }
        }

        $stmt->execute();

        if (!isset($data[$this->primaryKey])) {
            $data[$this->primaryKey] = (int) $this->db->lastInsertId();
        }

        return $this->mapToEntity($data);
    }

    /**
     * Update an existing entity in the database
     * 
     * @param array $data Entity data as associative array
     * @return object Updated entity
     */
    protected function update(array $data)
    {
        $id = $data[$this->primaryKey];

        // Automatically set updated_at timestamp if it exists in the columns
        if (in_array('updated_at', $this->columns) && !isset($data['updated_at'])) {
            $data['updated_at'] = date('Y-m-d H:i:s');
        }

        $sets = [];

        foreach ($data as $column => $value) {
            if ($column !== $this->primaryKey) {
                $sets[] = "$column = :$column";
            }
        }

        $setString = implode(', ', $sets);

        $stmt = $this->db->prepare("UPDATE {$this->table} SET $setString WHERE {$this->primaryKey} = :id");
        $stmt->bindValue(':id', $id);

        foreach ($data as $column => $value) {
            if ($column !== $this->primaryKey) {
                // Convert boolean values to integers for MySQL
                if (is_bool($value)) {
                    $stmt->bindValue(":$column", $value ? 1 : 0, \PDO::PARAM_INT);
                } else {
                    $stmt->bindValue(":$column", $value);
                }
            }
        }

        $stmt->execute();

        return $this->find($id);
    }

    /**
     * Begin a database transaction
     */
    public function beginTransaction()
    {
        $this->db->beginTransaction();
    }

    /**
     * Commit the current database transaction
     */
    public function commit()
    {
        $this->db->commit();
    }

    /**
     * Rollback the current database transaction
     */
    public function rollback()
    {
        $this->db->rollBack();
    }

    /**
     * Map database data to an entity object
     * 
     * @param array $data Database data as associative array
     * @return object Entity instance
     */
    protected function mapToEntity(array $data)
    {
        return new $this->entityClass($data);
    }

    /**
     * Map entity object to database data
     * 
     * @param object $entity Entity object to map
     * @return array Database data as associative array
     */
    protected function mapFromEntity($entity): array
    {
        if (method_exists($entity, 'toArray')) {
            return $entity->toArray();
        }

        $data = [];
        foreach ($this->columns as $column) {
            /**
             * Convert column name to getter method name
             * e.g., 'first_name' becomes 'getFirstName'
             */
            $getter = 'get' . ucfirst($column);
            if (method_exists($entity, $getter)) {
                $data[$column] = $entity->$getter();
            }
        }

        return $data;
    }
}
