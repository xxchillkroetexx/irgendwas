<?php

namespace SecretSanta\Database;

use SecretSanta\Config\Database;

abstract class DataMapper
{
    protected \mysqli $db;
    protected string $table;
    protected string $entityClass;
    protected array $columns = [];
    protected string $primaryKey = 'id';

    public function __construct()
    {
        $this->db = Database::getInstance()->getConnection();
    }

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

    public function findAll(array $orderBy = [], ?int $limit = null, ?int $offset = null)
    {
        return $this->findBy([], $orderBy, $limit, $offset);
    }

    public function save($entity)
    {
        $data = $this->mapFromEntity($entity);

        if (empty($data[$this->primaryKey])) {
            return $this->insert($data);
        } else {
            return $this->update($data);
        }
    }

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

    public function deleteById(int $id): bool
    {
        $stmt = $this->db->prepare("DELETE FROM {$this->table} WHERE {$this->primaryKey} = ?");
        $stmt->bind_param('i', $id);
        $stmt->execute();
        $affectedRows = $stmt->affected_rows;
        $stmt->close();

        return $affectedRows > 0;
    }

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

    public function beginTransaction()
    {
        $this->db->begin_transaction();
    }

    public function commit()
    {
        $this->db->commit();
    }

    public function rollback()
    {
        $this->db->rollback();
    }

    protected function mapToEntity(array $data)
    {
        return new $this->entityClass($data);
    }

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
