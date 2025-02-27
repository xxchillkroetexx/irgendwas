<?php
namespace controllers;

use core\Controller;
use models\User;

class ProfileController extends Controller {
    // Constructor - require authentication for all methods
    public function __construct() {
        $this->requireAuth();
    }
    
    // Show user profile
    public function show() {
        $user = $this->currentUser();
        
        $this->view('profile/show', [
            'pageTitle' => 'My Profile',
            'user' => $user,
            'csrf' => $this->generateCSRF()
        ]);
    }
    
    // Update user profile
    public function update() {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get form data
        $firstName = $_POST['first_name'] ?? '';
        $lastName = $_POST['last_name'] ?? '';
        $email = $_POST['email'] ?? '';
        $currentPassword = $_POST['current_password'] ?? '';
        $newPassword = $_POST['new_password'] ?? '';
        $confirmPassword = $_POST['confirm_password'] ?? '';
        
        // Validate input
        $errors = [];
        
        if (empty($firstName)) {
            $errors['first_name'] = 'First name is required';
        }
        
        if (empty($lastName)) {
            $errors['last_name'] = 'Last name is required';
        }
        
        if (empty($email)) {
            $errors['email'] = 'Email is required';
        } elseif (!filter_var($email, FILTER_VALIDATE_EMAIL)) {
            $errors['email'] = 'Invalid email format';
        } elseif ($email !== $user->getEmail()) {
            // Check if email is already in use by another user
            $userModel = new User();
            $existingUser = $userModel->findByEmail($email);
            if ($existingUser && $existingUser->getId() !== $user->getId()) {
                $errors['email'] = 'Email address is already in use';
            }
        }
        
        // Password validation - only if user is trying to change password
        $changePassword = false;
        if (!empty($newPassword) || !empty($confirmPassword)) {
            $changePassword = true;
            
            // Validate current password
            if (empty($currentPassword) || !$user->verifyPassword($currentPassword)) {
                $errors['current_password'] = 'Current password is incorrect';
            }
            
            if (empty($newPassword)) {
                $errors['new_password'] = 'New password is required';
            } elseif (strlen($newPassword) < 8) {
                $errors['new_password'] = 'New password must be at least 8 characters long';
            }
            
            if ($newPassword !== $confirmPassword) {
                $errors['confirm_password'] = 'New passwords do not match';
            }
        }
        
        if (!empty($errors)) {
            $this->view('profile/show', [
                'pageTitle' => 'My Profile',
                'user' => $user,
                'errors' => $errors,
                'csrf' => $this->generateCSRF(),
                'first_name' => $firstName,
                'last_name' => $lastName,
                'email' => $email
            ]);
            return;
        }
        
        // Update user data
        $userData = [
            'first_name' => $firstName,
            'last_name' => $lastName,
            'email' => $email
        ];
        
        // Only update password if provided
        if ($changePassword) {
            $userData['password'] = $newPassword;
        }
        
        $updatedUser = $user->update($userData);
        
        if ($updatedUser) {
            $this->flash('success', 'Profile updated successfully');
        } else {
            $this->flash('danger', 'Failed to update profile');
        }
        
        $this->redirect('/profile');
    }
}
