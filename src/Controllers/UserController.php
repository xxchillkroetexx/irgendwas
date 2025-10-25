<?php

namespace SecretSanta\Controllers;

use SecretSanta\Repositories\UserRepository;
use SecretSanta\Repositories\GroupRepository;
use SecretSanta\Repositories\GiftAssignmentRepository;

/**
 * User Controller
 * 
 * Handles user-specific functionality including profile management,
 * dashboard display, and user data API endpoints.
 * 
 * @package SecretSanta\Controllers
 */
class UserController extends BaseController
{
    /**
     * Display the user's dashboard
     * 
     * Shows groups the user belongs to and any gift assignments they have.
     * Requires authentication.
     * 
     * @return string HTML content
     */
    public function dashboard()
    {
        // Require authentication
        $this->requireAuth();

        $user = $this->auth->user();

        // Get groups the user is a member of
        $groupRepository = new GroupRepository();
        $groups = $groupRepository->findByUserId($user->getId());

        // Get gift assignments
        $assignmentRepository = new GiftAssignmentRepository();
        $assignments = $assignmentRepository->findByGiverId($user->getId());

        // Load relationships for assignments
        foreach ($assignments as $assignment) {
            $assignmentRepository->loadReceiver($assignment);

            // Load group for each assignment
            $group = $groupRepository->find($assignment->getGroupId());
            if ($group) {
                $assignment->setGroup($group);
            }
        }

        return $this->render('user/dashboard', [
            'user' => $user,
            'groups' => $groups,
            'assignments' => $assignments
        ]);
    }

    /**
     * Display the user profile page
     * 
     * Shows the profile information and allows for edits.
     * Requires authentication.
     * 
     * @return string HTML content
     */
    public function showProfile()
    {
        // Require authentication
        $this->requireAuth();

        $user = $this->auth->user();

        return $this->render('user/profile', [
            'user' => $user
        ]);
    }

    /**
     * Update the user's profile information
     * 
     * Handles name changes and password updates.
     * Requires authentication.
     * 
     * @return void
     */
    public function updateProfile()
    {
        // Require authentication
        $this->requireAuth();

        $user = $this->auth->user();

        $name = $this->request->getPostParam('name');
        $email = $this->request->getPostParam('email');
        $currentPassword = $this->request->getPostParam('current_password');
        $newPassword = $this->request->getPostParam('new_password');
        $passwordConfirm = $this->request->getPostParam('password_confirm');

        $userRepository = new UserRepository();

        // Handle password change if provided
        if (!empty($currentPassword) && !empty($newPassword)) {
            // Validate current password
            if (!password_verify($currentPassword, $user->getPassword())) {
                $this->session->setFlash('error', 'Current password is incorrect');
                $this->redirect('/user/profile');
                return;
            }

            // Validate new password
            if (strlen($newPassword) < 8) {
                $this->session->setFlash('error', 'New password must be at least 8 characters long');
                $this->redirect('/user/profile');
                return;
            }

            // Confirm passwords match
            if ($newPassword !== $passwordConfirm) {
                $this->session->setFlash('error', 'New passwords do not match');
                $this->redirect('/user/profile');
                return;
            }

            // Update password
            $user->setPassword(password_hash($newPassword, PASSWORD_DEFAULT));
        }

        // Handle name update
        if (!empty($name) && $name !== $user->getName()) {
            $user->setName($name);
        }

        // Handle email change
        if (!empty($email) && $email !== $user->getEmail()) {
            if (!filter_var($email, FILTER_VALIDATE_EMAIL)) {
                $this->session->setFlash('error', 'Invalid email address');
                $this->redirect('/user/profile');
                return;
            }

            if ($this->auth->requestEmailChange($user, $email)) {
                $this->session->setFlash('success', 'Email verification sent');
            } else {
                $this->session->setFlash('error', 'Email is already in use');
            }
            $this->redirect('/user/profile');
            return;
        }

        // Save other changes
        $userRepository->save($user);
        $this->session->setFlash('success', 'Profile updated successfully');
        $this->redirect('/user/profile');
    }

    /**
     * API endpoint to get user data
     * 
     * Returns user information in JSON format.
     * Requires authentication and only allows access to own data.
     * 
     * @param int $id The user ID to retrieve
     * @return string JSON response
     */
    public function apiGetUser($id)
    {
        // Require authentication
        $this->requireAuth();

        // Only allow access to the current user or administrators
        if ($id != $this->auth->userId()) {
            return $this->json(['error' => 'Unauthorized'], 403);
        }

        $userRepository = new UserRepository();
        $user = $userRepository->find($id);

        if (!$user) {
            return $this->json(['error' => 'User not found'], 404);
        }

        // Don't include sensitive data
        return $this->json([
            'id' => $user->getId(),
            'name' => $user->getName(),
            'email' => $user->getEmail()
        ]);
    }

    /**
     * Verify the email change request
     * 
     * Confirms the user's new email address using a verification token.
     * 
     * @param string $token The verification token
     * @return void
     */
    public function verifyEmail($token)
    {
        if ($this->auth->verifyEmailChange($token)) {
            $this->session->setFlash('success', 'Email verified successfully');
        } else {
            $this->session->setFlash('error', 'Email verification failed');
        }
        $this->redirect('/user/profile');
    }
}
