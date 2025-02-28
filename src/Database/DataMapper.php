<?php

namespace SecretSanta\Database;

use SecretSanta\Config\Database;

abstract class DataMapper {
    protected \PDO $db;
    protected string $table;
    protected string $entityClass;
    protected array $columns = [];
    protected string $primaryKey = 'id';
    
    public function __construct() {
        $this->db = Database::getInstance()->getConnection();
    }
    
    public function find(int $id) {
        $stmt = $this->db->prepare("SELECT * FROM {$this->table} WHERE {$this->primaryKey} = :id LIMIT 1");
        $stmt->execute(['id' => $id]);
        $data = $stmt->fetch();
        
        if (!$data) {
            return null;
        }
        
        return $this->mapToEntity($data);
    }
    
    public function findBy(array $criteria, array $orderBy = [], ?int $limit = null, ?int $offset = null) {
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
    
    public function findAll(array $orderBy = [], ?int $limit = null, ?int $offset = null) {
        return $this->findBy([], $orderBy, $limit, $offset);
    }
    
    public function save($entity) {
        $data = $this->mapFromEntity($entity);
        
        if (empty($data[$this->primaryKey])) {
            return $this->insert($data);
        } else {
            return $this->update($data);
        }
    }
    
    public function delete($entity): bool {
        $data = $this->mapFromEntity($entity);
        
        if (empty($data[$this->primaryKey])) {
            return false;
        }
        
        $stmt = $this->db->prepare("DELETE FROM {$this->table} WHERE {$this->primaryKey} = :id");
        $stmt->execute(['id' => $data[$this->primaryKey]]);
        
        return $stmt->rowCount() > 0;
    }
    
    public function deleteById(int $id): bool {
        $stmt = $this->db->prepare("DELETE FROM {$this->table} WHERE {$this->primaryKey} = :id");
        $stmt->execute(['id' => $id]);
        
        return $stmt->rowCount() > 0;
    }
    
    protected function insert(array $data) {
        $columns = array_keys(array_filter($data, function($key) {
            return $key !== $this->primaryKey || $data[$this->primaryKey] !== null;
        }, ARRAY_FILTER_USE_KEY));
        
        $placeholders = array_map(function($col) {
            return ":$col";
        }, $columns);
        
        $columnString = implode(', ', $columns);
        $placeholderString = implode(', ', $placeholders);
        
        $stmt = $this->db->prepare("INSERT INTO {$this->table} ($columnString) VALUES ($placeholderString)");
        
        foreach ($columns as $column) {
            $stmt->bindValue(":$column", $data[$column]);
        }
        
        $stmt->execute();
        
        if (!isset($data[$this->primaryKey])) {
            $data[$this->primaryKey] = (int) $this->db->lastInsertId();
        }
        
        return $this->mapToEntity($data);
    }
    
    protected function update(array $data) {
        $id = $data[$this->primaryKey];
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
                $stmt->bindValue(":$column", $value);
            }
        }
        
        $stmt->execute();
        
        return $this->find($id);
    }
    
    public function beginTransaction() {
        $this->db->beginTransaction();
    }
    
    public function commit() {
        $this->db->commit();
    }
    
    public function rollback() {
        $this->db->rollBack();
    }
    
    protected function mapToEntity(array $data) {
        return new $this->entityClass($data);
    }
    
    protected function mapFromEntity($entity): array {
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