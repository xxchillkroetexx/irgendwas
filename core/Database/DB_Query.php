<?php
namespace core\Database;

class DB_Query {
    private $db;
    private $table;
    private $select = '*';
    private $where = [];
    private $bindings = [];
    private $order = [];
    private $limit = null;
    private $offset = null;
    private $joins = [];
    private $groupBy = null;
    private $having = [];
    
    public function __construct(Projekt_DB $db, $table) {
        $this->db = $db;
        $this->table = $table;
    }
    
    // Set the columns to select
    public function select($columns = '*') {
        $this->select = is_array($columns) ? implode(', ', $columns) : $columns;
        return $this;
    }
    
    // Add a where clause
    public function where($column, $operator = null, $value = null) {
        // If only two parameters are provided, assume equality comparison
        if ($value === null) {
            $value = $operator;
            $operator = '=';
        }
        
        $placeholder = ':' . count($this->bindings);
        $this->where[] = "$column $operator $placeholder";
        $this->bindings[$placeholder] = $value;
        
        return $this;
    }
    
    // Add a where IN clause
    public function whereIn($column, array $values) {
        $placeholders = [];
        
        foreach ($values as $value) {
            $placeholder = ':' . count($this->bindings);
            $placeholders[] = $placeholder;
            $this->bindings[$placeholder] = $value;
        }
        
        $this->where[] = "$column IN (" . implode(', ', $placeholders) . ")";
        
        return $this;
    }
    
    // Add a raw where clause
    public function whereRaw($sql, $bindings = []) {
        $this->where[] = $sql;
        
        foreach ($bindings as $value) {
            $placeholder = ':' . count($this->bindings);
            $this->bindings[$placeholder] = $value;
        }
        
        return $this;
    }
    
    // Add join clause
    public function join($table, $first, $operator, $second) {
        $this->joins[] = "JOIN $table ON $first $operator $second";
        return $this;
    }
    
    // Add left join clause
    public function leftJoin($table, $first, $operator, $second) {
        $this->joins[] = "LEFT JOIN $table ON $first $operator $second";
        return $this;
    }
    
    // Add right join clause
    public function rightJoin($table, $first, $operator, $second) {
        $this->joins[] = "RIGHT JOIN $table ON $first $operator $second";
        return $this;
    }
    
    // Set the order by clause
    public function orderBy($column, $direction = 'ASC') {
        $this->order[] = "$column $direction";
        return $this;
    }
    
    // Set the limit clause
    public function limit($limit) {
        $this->limit = (int) $limit;
        return $this;
    }
    
    // Set the offset clause
    public function offset($offset) {
        $this->offset = (int) $offset;
        return $this;
    }
    
    // Set the group by clause
    public function groupBy($column) {
        $this->groupBy = $column;
        return $this;
    }
    
    // Add a having clause
    public function having($column, $operator = null, $value = null) {
        // If only two parameters are provided, assume equality comparison
        if ($value === null) {
            $value = $operator;
            $operator = '=';
        }
        
        $placeholder = ':having_' . count($this->bindings);
        $this->having[] = "$column $operator $placeholder";
        $this->bindings[$placeholder] = $value;
        
        return $this;
    }
    
    // Build the SQL query
    public function buildQuery($type = 'SELECT') {
        $query = '';
        
        if ($type === 'SELECT') {
            $query = "SELECT {$this->select} FROM {$this->table}";
            
            // Add joins
            if (!empty($this->joins)) {
                $query .= ' ' . implode(' ', $this->joins);
            }
            
            // Add where conditions
            if (!empty($this->where)) {
                $query .= ' WHERE ' . implode(' AND ', $this->where);
            }
            
            // Add group by
            if ($this->groupBy !== null) {
                $query .= ' GROUP BY ' . $this->groupBy;
            }
            
            // Add having
            if (!empty($this->having)) {
                $query .= ' HAVING ' . implode(' AND ', $this->having);
            }
            
            // Add order by
            if (!empty($this->order)) {
                $query .= ' ORDER BY ' . implode(', ', $this->order);
            }
            
            // Add limit and offset
            if ($this->limit !== null) {
                $query .= ' LIMIT ' . $this->limit;
                
                if ($this->offset !== null) {
                    $query .= ' OFFSET ' . $this->offset;
                }
            }
        }
        
        return $query;
    }
    
    // Execute the query and get all results
    public function get() {
        $query = $this->buildQuery();
        return $this->db->fetchAll($query, $this->bindings);
    }
    
    // Execute the query and get the first result
    public function first() {
        $this->limit(1);
        $query = $this->buildQuery();
        return $this->db->fetch($query, $this->bindings);
    }
    
    // Execute the query and get a single column value
    public function value($column) {
        $this->select($column);
        $query = $this->buildQuery();
        return $this->db->fetchColumn($query, $this->bindings);
    }
    
    // Count the number of rows
    public function count($column = '*') {
        $this->select("COUNT($column) as count");
        $query = $this->buildQuery();
        return (int) $this->db->fetchColumn($query, $this->bindings);
    }
    
    // Insert data into the table
    public function insert(array $data) {
        $columns = array_keys($data);
        $placeholders = [];
        $values = [];
        
        foreach ($columns as $column) {
            $placeholder = ':' . $column;
            $placeholders[] = $placeholder;
            $values[$placeholder] = $data[$column];
        }
        
        $columnsString = implode(', ', $columns);
        $placeholdersString = implode(', ', $placeholders);
        
        $query = "INSERT INTO {$this->table} ($columnsString) VALUES ($placeholdersString)";
        $this->db->execute($query, $values);
        
        return $this->db->lastInsertId();
    }
    
    // Update data in the table
    public function update(array $data) {
        $setParts = [];
        $values = [];
        
        foreach ($data as $column => $value) {
            $placeholder = ':' . $column;
            $setParts[] = "$column = $placeholder";
            $values[$placeholder] = $value;
        }
        
        $setString = implode(', ', $setParts);
        
        $query = "UPDATE {$this->table} SET $setString";
        
        // Add where conditions
        if (!empty($this->where)) {
            $query .= ' WHERE ' . implode(' AND ', $this->where);
            $values = array_merge($values, $this->bindings);
        }
        
        return $this->db->execute($query, $values) !== false;
    }
    
    // Delete rows from the table
    public function delete() {
        $query = "DELETE FROM {$this->table}";
        
        // Add where conditions
        if (!empty($this->where)) {
            $query .= ' WHERE ' . implode(' AND ', $this->where);
            return $this->db->execute($query, $this->bindings) !== false;
        }
        
        // Prevent accidental deletion of all records
        throw new \Exception("Delete operation requires where conditions");
    }
}