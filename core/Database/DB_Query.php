<?php
namespace core\Database;

/**
 * SQL Query Builder class
 */
class DB_Query {
    private Projekt_DB $db;
    private string $table;
    private string $select = '*';
    private array $where = [];
    private array $bindings = [];
    private array $order = [];
    private ?int $limit = null;
    private ?int $offset = null;
    private array $joins = [];
    private ?string $groupBy = null;
    private array $having = [];
    private array $unions = [];
    
    /**
     * Constructor
     * 
     * @param Projekt_DB $db Database instance
     * @param string $table Table name
     */
    public function __construct(Projekt_DB $db, string $table) {
        $this->db = $db;
        $this->table = $table;
    }
    
    /**
     * Set the columns to select
     * 
     * @param string|array $columns Columns to select
     * @return DB_Query Self for method chaining
     */
    public function select($columns = '*'): DB_Query {
        $this->select = is_array($columns) ? implode(', ', $columns) : $columns;
        return $this;
    }
    
    /**
     * Add a where clause
     * 
     * @param string|array $column Column name or conditions array
     * @param mixed $operator Operator or value if operator is omitted
     * @param mixed $value Value (optional if $operator is the value)
     * @return DB_Query Self for method chaining
     */
    public function where($column, $operator = null, $value = null): DB_Query {
        // Handle array of conditions
        if (is_array($column)) {
            foreach ($column as $key => $val) {
                $this->where($key, '=', $val);
            }
            return $this;
        }
        
        // If only two parameters are provided, assume equality comparison
        if ($value === null) {
            $value = $operator;
            $operator = '=';
        }
        
        // Handle NULL values for IS NULL or IS NOT NULL
        if ($value === null) {
            if (strtoupper($operator) === 'IS' || strtoupper($operator) === 'IS NOT') {
                $this->where[] = "$column $operator NULL";
                return $this;
            }
        }
        
        $placeholder = ':where_' . count($this->bindings);
        $this->where[] = "$column $operator $placeholder";
        $this->bindings[$placeholder] = $value;
        
        return $this;
    }
    
    /**
     * Add a where IN clause
     * 
     * @param string $column Column name
     * @param array $values Array of values
     * @return DB_Query Self for method chaining
     */
    public function whereIn(string $column, array $values): DB_Query {
        if (empty($values)) {
            // Handle empty arrays specially
            $this->where[] = "0 = 1"; // This will always be false
            return $this;
        }
        
        $placeholders = [];
        
        foreach ($values as $value) {
            $placeholder = ':wherein_' . count($this->bindings);
            $placeholders[] = $placeholder;
            $this->bindings[$placeholder] = $value;
        }
        
        $this->where[] = "$column IN (" . implode(', ', $placeholders) . ")";
        
        return $this;
    }
    
    /**
     * Add a where NOT IN clause
     * 
     * @param string $column Column name
     * @param array $values Array of values
     * @return DB_Query Self for method chaining
     */
    public function whereNotIn(string $column, array $values): DB_Query {
        if (empty($values)) {
            // If values array is empty, condition is always true
            $this->where[] = "1 = 1";
            return $this;
        }
        
        $placeholders = [];
        
        foreach ($values as $value) {
            $placeholder = ':wherenotin_' . count($this->bindings);
            $placeholders[] = $placeholder;
            $this->bindings[$placeholder] = $value;
        }
        
        $this->where[] = "$column NOT IN (" . implode(', ', $placeholders) . ")";
        
        return $this;
    }
    
    /**
     * Add a where BETWEEN clause
     * 
     * @param string $column Column name
     * @param mixed $min Minimum value
     * @param mixed $max Maximum value
     * @return DB_Query Self for method chaining
     */
    public function whereBetween(string $column, $min, $max): DB_Query {
        $minPlaceholder = ':between_min_' . count($this->bindings);
        $maxPlaceholder = ':between_max_' . count($this->bindings) + 1;
        
        $this->where[] = "$column BETWEEN $minPlaceholder AND $maxPlaceholder";
        $this->bindings[$minPlaceholder] = $min;
        $this->bindings[$maxPlaceholder] = $max;
        
        return $this;
    }
    
    /**
     * Add a WHERE column IS NULL clause
     * 
     * @param string $column Column name
     * @return DB_Query Self for method chaining
     */
    public function whereNull(string $column): DB_Query {
        $this->where[] = "$column IS NULL";
        return $this;
    }
    
    /**
     * Add a WHERE column IS NOT NULL clause
     * 
     * @param string $column Column name
     * @return DB_Query Self for method chaining
     */
    public function whereNotNull(string $column): DB_Query {
        $this->where[] = "$column IS NOT NULL";
        return $this;
    }
    
    /**
     * Add a raw where clause
     * 
     * @param string $sql Raw SQL condition
     * @param array $bindings Values to bind to the query
     * @return DB_Query Self for method chaining
     */
    public function whereRaw(string $sql, array $bindings = []): DB_Query {
        $this->where[] = $sql;
        
        foreach ($bindings as $value) {
            $placeholder = ':whereraw_' . count($this->bindings);
            $this->bindings[$placeholder] = $value;
        }
        
        return $this;
    }
    
    /**
     * Add OR condition
     * 
     * @param string $column Column name
     * @param mixed $operator Operator or value
     * @param mixed $value Value (optional)
     * @return DB_Query Self for method chaining
     */
    public function orWhere($column, $operator = null, $value = null): DB_Query {
        // If only two parameters are provided, assume equality comparison
        if ($value === null) {
            $value = $operator;
            $operator = '=';
        }
        
        $placeholder = ':orwhere_' . count($this->bindings);
        
        // If there are existing where conditions, use OR
        if (!empty($this->where)) {
            $this->where[] = "OR $column $operator $placeholder";
        } else {
            $this->where[] = "$column $operator $placeholder";
        }
        
        $this->bindings[$placeholder] = $value;
        
        return $this;
    }
    
    /**
     * Add join clause
     * 
     * @param string $table Table to join
     * @param string $first First column
     * @param string $operator Comparison operator
     * @param string $second Second column
     * @return DB_Query Self for method chaining
     */
    public function join(string $table, string $first, string $operator, string $second): DB_Query {
        $this->joins[] = "JOIN $table ON $first $operator $second";
        return $this;
    }
    
    /**
     * Add left join clause
     * 
     * @param string $table Table to join
     * @param string $first First column
     * @param string $operator Comparison operator
     * @param string $second Second column
     * @return DB_Query Self for method chaining
     */
    public function leftJoin(string $table, string $first, string $operator, string $second): DB_Query {
        $this->joins[] = "LEFT JOIN $table ON $first $operator $second";
        return $this;
    }
    
    /**
     * Add right join clause
     * 
     * @param string $table Table to join
     * @param string $first First column
     * @param string $operator Comparison operator
     * @param string $second Second column
     * @return DB_Query Self for method chaining
     */
    public function rightJoin(string $table, string $first, string $operator, string $second): DB_Query {
        $this->joins[] = "RIGHT JOIN $table ON $first $operator $second";
        return $this;
    }
    
    /**
     * Add a raw join clause
     * 
     * @param string $sql Raw SQL join clause
     * @return DB_Query Self for method chaining
     */
    public function joinRaw(string $sql): DB_Query {
        $this->joins[] = $sql;
        return $this;
    }
    
    /**
     * Set the order by clause
     * 
     * @param string $column Column name
     * @param string $direction Sort direction ('ASC' or 'DESC')
     * @return DB_Query Self for method chaining
     */
    public function orderBy(string $column, string $direction = 'ASC'): DB_Query {
        $direction = strtoupper($direction);
        if ($direction !== 'ASC' && $direction !== 'DESC') {
            $direction = 'ASC';
        }
        $this->order[] = "$column $direction";
        return $this;
    }
    
    /**
     * Add random ordering
     * 
     * @return DB_Query Self for method chaining
     */
    public function orderByRandom(): DB_Query {
        $this->order[] = "RAND()";
        return $this;
    }
    
    /**
     * Set the limit clause
     * 
     * @param int $limit Maximum number of records
     * @return DB_Query Self for method chaining
     */
    public function limit(int $limit): DB_Query {
        $this->limit = $limit;
        return $this;
    }
    
    /**
     * Set the offset clause
     * 
     * @param int $offset Number of records to skip
     * @return DB_Query Self for method chaining
     */
    public function offset(int $offset): DB_Query {
        $this->offset = $offset;
        return $this;
    }
    
    /**
     * Add pagination
     * 
     * @param int $page Page number (1-based)
     * @param int $perPage Items per page
     * @return DB_Query Self for method chaining
     */
    public function paginate(int $page, int $perPage = 15): DB_Query {
        $offset = ($page - 1) * $perPage;
        $this->limit($perPage);
        $this->offset($offset);
        return $this;
    }
    
    /**
     * Set the group by clause
     * 
     * @param string|array $columns Column(s) to group by
     * @return DB_Query Self for method chaining
     */
    public function groupBy($columns): DB_Query {
        if (is_array($columns)) {
            $this->groupBy = implode(', ', $columns);
        } else {
            $this->groupBy = $columns;
        }
        return $this;
    }
    
    /**
     * Add a having clause
     * 
     * @param string $column Column name
     * @param mixed $operator Operator or value
     * @param mixed $value Value (optional)
     * @return DB_Query Self for method chaining
     */
    public function having(string $column, $operator = null, $value = null): DB_Query {
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
    
    /**
     * Add a UNION clause
     * 
     * @param DB_Query $query Query to union
     * @param bool $all Whether to use UNION ALL
     * @return DB_Query Self for method chaining
     */
    public function union(DB_Query $query, bool $all = false): DB_Query {
        $type = $all ? 'UNION ALL' : 'UNION';
        $this->unions[] = [
            'type' => $type,
            'query' => $query
        ];
        return $this;
    }
    
    /**
     * Build the SQL query
     * 
     * @param string $type Query type (SELECT, INSERT, UPDATE, DELETE)
     * @return string SQL query
     */
    public function buildQuery(string $type = 'SELECT'): string {
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
            
            // Add unions
            foreach ($this->unions as $union) {
                $unionQuery = $union['query']->buildQuery();
                $query = "($query) {$union['type']} ($unionQuery)";
            }
        }
        
        return $query;
    }
    
    /**
     * Execute the query and get all results
     * 
     * @param array $columns Optional columns to select
     * @return array Results as associative array
     */
    public function get(array $columns = []): array {
        if (!empty($columns)) {
            $this->select($columns);
        }
        
        $query = $this->buildQuery();
        return $this->db->fetchAll($query, $this->bindings);
    }
    
    /**
     * Execute the query and get the first result
     * 
     * @param array $columns Optional columns to select
     * @return array|false First result row or false if none
     */
    public function first(array $columns = []) {
        if (!empty($columns)) {
            $this->select($columns);
        }
        
        $this->limit(1);
        $query = $this->buildQuery();
        return $this->db->fetch($query, $this->bindings);
    }
    
    /**
     * Execute the query and get a single column value
     * 
     * @param string $column Column name
     * @return mixed Column value
     */
    public function value(string $column) {
        $this->select($column);
        $query = $this->buildQuery();
        return $this->db->fetchColumn($query, $this->bindings);
    }
    
    /**
     * Count the number of rows
     * 
     * @param string $column Column name
     * @return int Row count
     */
    public function count(string $column = '*'): int {
        $this->select("COUNT($column) as count");
        $query = $this->buildQuery();
        return (int) $this->db->fetchColumn($query, $this->bindings);
    }
    
    /**
     * Insert data into the table
     * 
     * @param array $data Data to insert
     * @return int Last insert ID
     */
    public function insert(array $data): int {
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
    
    /**
     * Update data in the table
     * 
     * @param array $data Data to update
     * @return bool Success status
     */
    public function update(array $data): bool {
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
    
    /**
     * Delete rows from the table
     * 
     * @return bool Success status
     * @throws \Exception If no where conditions are set
     */
    public function delete(): bool {
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