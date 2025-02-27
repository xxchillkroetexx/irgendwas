<?php
namespace Appmodels;

use core\Database\Projekt_DB;

class User {
    private $db;
    
    // User properties
    private $id;
    private $email;
    private $password;
    private $firstName;
    private $lastName;
    private $resetToken;
    private $resetTokenExpiry;
    private $createdAt;
    private $updatedAt;
    
    public function __construct($data = null) {
        $this->db = Projekt_DB::getInstance();
        
        if ($data) {
            $this->mapData($data);
        }
    }
    
    // Map data from array or object to properties
    private function mapData($data) {
        $data = (array) $data;
        
        $this->id = $data['id'] ?? null;
        $this->email = $data['email'] ?? null;
        $this->password = $data['password'] ?? null;
        $this->firstName = $data['first_name'] ?? null;
        $this->lastName = $data['last_name'] ?? null;
        $this->resetToken = $data['reset_token'] ?? null;
        $this->resetTokenExpiry = $data['reset_token_expiry'] ?? null;
        $this->createdAt = $data['created_at'] ?? null;
        $this->updatedAt = $data['updated_at'] ?? null;
    }
    
    // Find user by ID
    public function findById($id) {
        $user = $this->db->table('users')
            ->where('id', $id)
            ->first();
            
        if ($user) {
            return new self($user);
        }
        
        return null;
    }
    
    // Find user by email
    public function findByEmail($email) {
        $user = $this->db->table('users')
            ->where('email', $email)
            ->first();
            
        if ($user) {
            return new self($user);
        }
        
        return null;
    }
    
    // Find user by reset token
    public function findByResetToken($token) {
        $user = $this->db->table('users')
            ->where('reset_token', $token)
            ->where('reset_token_expiry', '>', date('Y-m-d H:i:s'))
            ->first();
            
        if ($user) {
            return new self($user);
        }
        
        return null;
    }
    
    // Create a new user
    public function create($data) {
        // Hash password
        $data['password'] = password_hash($data['password'], PASSWORD_BCRYPT);
        
        $userId = $this->db->table('users')->insert([
            'email' => $data['email'],
            'password' => $data['password'],
            'first_name' => $data['first_name'],
            'last_name' => $data['last_name']
        ]);
        
        return $this->findById($userId);
    }
    
    // Update user
    public function update($data) {
        // Only update password if provided
        if (isset($data['password']) && !empty($data['password'])) {
            $data['password'] = password_hash($data['password'], PASSWORD_BCRYPT);
        } else {
            unset($data['password']);
        }
        
        $this->db->table('users')
            ->where('id', $this->id)
            ->update($data);
            
        return $this->findById($this->id);
    }
    
    // Create password reset token
    public function generateResetToken() {
        $token = bin2hex(random_bytes(32));
        $expiry = date('Y-m-d H:i:s', strtotime('+1 hour'));
        
        $this->db->table('users')
            ->where('id', $this->id)
            ->update([
                'reset_token' => $token,
                'reset_token_expiry' => $expiry
            ]);
            
        return $token;
    }
    
    // Clear password reset token
    public function clearResetToken() {
        $this->db->table('users')
            ->where('id', $this->id)
            ->update([
                'reset_token' => null,
                'reset_token_expiry' => null
            ]);
    }
    
    // Verify password
    public function verifyPassword($password) {
        return password_verify($password, $this->password);
    }
    
    // Get groups that user belongs to
    public function getGroups() {
        // For now, just return an empty array to avoid errors
        return [];
    }
    
    // Get groups that user administers
    public function getAdminGroups() {
        // For now, just return an empty array to avoid errors
        return [];
    }
    
    // Getters
    public function getId() {
        return $this->id;
    }
    
    public function getEmail() {
        return $this->email;
    }
    
    public function getFirstName() {
        return $this->firstName;
    }
    
    public function getLastName() {
        return $this->lastName;
    }
    
    public function getFullName() {
        return $this->firstName . ' ' . $this->lastName;
    }
    
    public function getResetToken() {
        return $this->resetToken;
    }
    
    public function getResetTokenExpiry() {
        return $this->resetTokenExpiry;
    }
    
    public function getCreatedAt() {
        return $this->createdAt;
    }
    
    public function getUpdatedAt() {
        return $this->updatedAt;
    }
}
