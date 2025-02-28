<?php
namespace controllers;

use core\Controller;
use models\Group;
use models\User;
use models\Wishlist;
use utils\EmailService;

class GroupController extends Controller {
    // Constructor - require authentication for all methods
    public function __construct() {
        $this->requireAuth();
    }
    
    // List user's groups
    public function index() {
        $user = $this->currentUser();
        
        // Get groups user is a member of
        $groups = $user->getGroups();
        
        // Get groups user administers
        $adminGroups = $user->getAdminGroups();
        
        $this->view('groups/index', [
            'pageTitle' => 'My Groups',
            'groups' => $groups,
            'adminGroups' => $adminGroups
        ]);
    }
    
    // Show group creation form
    public function create() {
        $this->view('groups/create', [
            'pageTitle' => 'Create Group',
            'csrf' => $this->generateCSRF()
        ]);
    }
    
    // Process group creation
    public function store() {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get form data
        $name = $_POST['name'] ?? '';
        $description = $_POST['description'] ?? '';
        $joinDeadline = !empty($_POST['join_deadline']) ? $_POST['join_deadline'] : null;
        $drawDate = !empty($_POST['draw_date']) ? $_POST['draw_date'] : null;
        $wishlistVisibility = $_POST['wishlist_visibility'] ?? 'all';

        // Validate input
        $errors = [];

        if (empty($name)) {
            $errors['name'] = 'Group name is required';
        }

        // Validate dates if provided
        if (!empty($joinDeadline) && !strtotime($joinDeadline)) {
            $errors['join_deadline'] = 'Invalid join deadline date';
        }

        if (!empty($drawDate) && !strtotime($drawDate)) {
            $errors['draw_date'] = 'Invalid draw date';
        }

        if (!empty($joinDeadline) && !empty($drawDate)) {
            if (strtotime($joinDeadline) > strtotime($drawDate)) {
                $errors['draw_date'] = 'Draw date must be after join deadline';
            }
        }

        if (!empty($errors)) {
            $this->view('groups/create', [
                'pageTitle' => 'Create Group',
                'csrf' => $this->generateCSRF(),
                'errors' => $errors,
                'name' => $name,
                'description' => $description,
                'join_deadline' => $joinDeadline,
                'draw_date' => $drawDate,
                'wishlist_visibility' => $wishlistVisibility
            ]);
            return;
        }
        
        // Create group
        $groupModel = new Group();
        $group = $groupModel->create([
            'name' => $name,
            'description' => $description,
            'admin_id' => $user->getId(),
            'join_deadline' => $joinDeadline,
            'draw_date' => $drawDate,
            'wishlist_visibility' => $wishlistVisibility
        ]);
        
        if (!$group) {
            $this->flash('danger', 'Failed to create group');
            $this->redirect('/groups');
            return;
        }
        
        // Create wishlist for user in this group
        $wishlistModel = new Wishlist();
        $wishlistModel->create($user->getId(), $group->getId());
        
        $this->flash('success', 'Group created successfully');
        $this->redirect('/groups/' . $group->getId());
    }
    
    // Show group details
    public function show($id) {
        $user = $this->currentUser();
        
        // Get group
        $groupModel = new Group();
        $group = $groupModel->findById($id);
        
        if (!$group) {
            $this->flash('danger', 'Group not found');
            $this->redirect('/groups');
            return;
        }
        
        // Check if user is member or admin
        if (!$group->isMember($user->getId()) && !$group->isAdmin($user->getId())) {
            $this->flash('danger', 'You are not a member of this group');
            $this->redirect('/groups');
            return;
        }
        
        // Get group members
        $members = $group->getMembers();
        
        // Get user's assignment if group is drawn
        $assignment = null;
        if ($group->getIsDrawn()) {
            $assignment = $group->getUserAssignment($user->getId());
        }
        
        // Get user's wishlist
        $wishlistModel = new Wishlist();
        $wishlist = $wishlistModel->findByUserAndGroup($user->getId(), $group->getId());
        
        // Get invitations if user is admin
        $invitations = [];
        if ($group->isAdmin($user->getId())) {
            $invitations = $group->getInvitations();
        }
        
        $this->view('groups/show', [
            'pageTitle' => $group->getName(),
            'group' => $group,
            'members' => $members,
            'assignment' => $assignment,
            'wishlist' => $wishlist,
            'invitations' => $invitations,
            'isAdmin' => $group->isAdmin($user->getId()),
            'csrf' => $this->generateCSRF()
        ]);
    }
    
    // Show group edit form
    public function edit($id) {
        $user = $this->currentUser();
        
        // Get group
        $groupModel = new Group();
        $group = $groupModel->findById($id);
        
        if (!$group) {
            $this->flash('danger', 'Group not found');
            $this->redirect('/groups');
            return;
        }
        
        // Check if user is admin
        if (!$group->isAdmin($user->getId())) {
            $this->flash('danger', 'You do not have permission to edit this group');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        $this->view('groups/edit', [
            'pageTitle' => 'Edit ' . $group->getName(),
            'group' => $group,
            'csrf' => $this->generateCSRF()
        ]);
    }
    
    // Process group update
    public function update($id) {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get group
        $groupModel = new Group();
        $group = $groupModel->findById($id);
        
        if (!$group) {
            $this->flash('danger', 'Group not found');
            $this->redirect('/groups');
            return;
        }
        
        // Check if user is admin
        if (!$group->isAdmin($user->getId())) {
            $this->flash('danger', 'You do not have permission to edit this group');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Get form data
        $name = $_POST['name'] ?? '';
        $description = $_POST['description'] ?? '';
        $joinDeadline = $_POST['join_deadline'] ?? null;
        $drawDate = $_POST['draw_date'] ?? null;
        $customEmailTemplate = $_POST['custom_email_template'] ?? null;
        $wishlistVisibility = $_POST['wishlist_visibility'] ?? 'all';
        
        // Validate input
        $errors = [];
        
        if (empty($name)) {
            $errors['name'] = 'Group name is required';
        }
        
        // Validate dates
        if (!empty($joinDeadline) && !strtotime($joinDeadline)) {
            $errors['join_deadline'] = 'Invalid join deadline date';
        }
        
        if (!empty($drawDate) && !strtotime($drawDate)) {
            $errors['draw_date'] = 'Invalid draw date';
        }
        
        if (!empty($joinDeadline) && !empty($drawDate)) {
            if (strtotime($joinDeadline) > strtotime($drawDate)) {
                $errors['draw_date'] = 'Draw date must be after join deadline';
            }
        }
        
        if (!empty($errors)) {
            $this->view('groups/edit', [
                'pageTitle' => 'Edit ' . $group->getName(),
                'group' => $group,
                'csrf' => $this->generateCSRF(),
                'errors' => $errors,
                'name' => $name,
                'description' => $description,
                'join_deadline' => $joinDeadline,
                'draw_date' => $drawDate,
                'custom_email_template' => $customEmailTemplate,
                'wishlist_visibility' => $wishlistVisibility
            ]);
            return;
        }
        
        // Update group
        $group->update([
            'name' => $name,
            'description' => $description,
            'join_deadline' => $joinDeadline,
            'draw_date' => $drawDate,
            'custom_email_template' => $customEmailTemplate,
            'wishlist_visibility' => $wishlistVisibility
        ]);
        
        $this->flash('success', 'Group updated successfully');
        $this->redirect('/groups/' . $id);
    }
    
    // Delete group
    public function delete($id) {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get group
        $groupModel = new Group();
        $group = $groupModel->findById($id);
        
        if (!$group) {
            $this->flash('danger', 'Group not found');
            $this->redirect('/groups');
            return;
        }
        
        // Check if user is admin
        if (!$group->isAdmin($user->getId())) {
            $this->flash('danger', 'You do not have permission to delete this group');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Delete group
        $group->delete();
        
        $this->flash('success', 'Group deleted successfully');
        $this->redirect('/groups');
    }
    
    // Perform the draw
    public function draw($id) {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get group
        $groupModel = new Group();
        $group = $groupModel->findById($id);
        
        if (!$group) {
            $this->flash('danger', 'Group not found');
            $this->redirect('/groups');
            return;
        }
        
        // Check if user is admin
        if (!$group->isAdmin($user->getId())) {
            $this->flash('danger', 'You do not have permission to perform the draw');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Check if group is already drawn
        if ($group->getIsDrawn()) {
            $this->flash('info', 'The draw has already been performed for this group');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Perform draw
        $success = $group->performDraw();
        
        if (!$success) {
            $this->flash('danger', 'Could not perform the draw. Please make sure there are at least 2 members in the group.');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Send assignment emails
        $emailService = new EmailService();
        $members = $group->getMembers();
        
        foreach ($members as $member) {
            $assignment = $group->getUserAssignment($member->getId());
            if ($assignment) {
                $emailService->sendAssignmentNotification($member, $assignment, $group);
            }
        }
        
        $this->flash('success', 'Draw completed and assignments sent by email');
        $this->redirect('/groups/' . $id);
    }
    
    // Redo the draw
    public function redraw($id) {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get group
        $groupModel = new Group();
        $group = $groupModel->findById($id);
        
        if (!$group) {
            $this->flash('danger', 'Group not found');
            $this->redirect('/groups');
            return;
        }
        
        // Check if user is admin
        if (!$group->isAdmin($user->getId())) {
            $this->flash('danger', 'You do not have permission to redo the draw');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Perform redraw
        $success = $group->redraw();
        
        if (!$success) {
            $this->flash('danger', 'Could not perform the redraw. Please make sure there are at least 2 members in the group.');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Send assignment emails
        $emailService = new EmailService();
        $members = $group->getMembers();
        
        foreach ($members as $member) {
            $assignment = $group->getUserAssignment($member->getId());
            if ($assignment) {
                $emailService->sendAssignmentNotification($member, $assignment, $group);
            }
        }
        
        $this->flash('success', 'Draw redone and new assignments sent by email');
        $this->redirect('/groups/' . $id);
    }
    
    // Send invitations
    public function sendInvites($id) {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get group
        $groupModel = new Group();
        $group = $groupModel->findById($id);
        
        if (!$group) {
            $this->flash('danger', 'Group not found');
            $this->redirect('/groups');
            return;
        }
        
        // Check if user is admin
        if (!$group->isAdmin($user->getId())) {
            $this->flash('danger', 'You do not have permission to send invitations');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Get email addresses
        $emails = $_POST['emails'] ?? '';
        $emailList = array_map('trim', explode(',', $emails));
        $emailList = array_filter($emailList, function($email) {
            return !empty($email) && filter_var($email, FILTER_VALIDATE_EMAIL);
        });
        
        if (empty($emailList)) {
            $this->flash('danger', 'Please enter at least one valid email address');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Send invitations
        $emailService = new EmailService();
        $count = 0;
        
        foreach ($emailList as $email) {
            $token = $group->inviteMember($email);
            if ($token) {
                $emailService->sendInvitation($email, $group, $token, $user);
                $count++;
            }
        }
        
        $this->flash('success', $count . ' invitation(s) sent successfully');
        $this->redirect('/groups/' . $id);
    }
    
    // Accept invitation
    public function acceptInvitation($token) {
        $user = $this->currentUser();
        
        // Find invitation
        $groupModel = new Group();
        $invitation = $groupModel->findInvitationByToken($token);
        
        if (!$invitation) {
            $this->flash('danger', 'Invalid or expired invitation');
            $this->redirect('/groups');
            return;
        }
        
        // Get group
        $group = $groupModel->findById($invitation['group_id']);
        
        if (!$group) {
            $this->flash('danger', 'Group not found');
            $this->redirect('/groups');
            return;
        }
        
        // Accept invitation
        $success = $group->acceptInvitation($token, $user->getId());
        
        if (!$success) {
            $this->flash('danger', 'Could not accept invitation');
            $this->redirect('/groups');
            return;
        }
        
        // Create wishlist for user in this group
        $wishlistModel = new Wishlist();
        $wishlistModel->create($user->getId(), $group->getId());
        
        $this->flash('success', 'You have joined the group "' . $group->getName() . '"');
        $this->redirect('/groups/' . $group->getId());
    }
    
    // Resend assignment email
    public function resendAssignmentEmail($id, $memberId) {
        $this->validateCSRF();
        
        $user = $this->currentUser();
        
        // Get group
        $groupModel = new Group();
        $group = $groupModel->findById($id);
        
        if (!$group) {
            $this->flash('danger', 'Group not found');
            $this->redirect('/groups');
            return;
        }
        
        // Check if user is admin
        if (!$group->isAdmin($user->getId())) {
            $this->flash('danger', 'You do not have permission to resend emails');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Check if group is drawn
        if (!$group->getIsDrawn()) {
            $this->flash('danger', 'The draw has not been performed yet');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Get member
        $userModel = new User();
        $member = $userModel->findById($memberId);
        
        if (!$member || !$group->isMember($memberId)) {
            $this->flash('danger', 'Member not found in this group');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Get assignment
        $assignment = $group->getUserAssignment($memberId);
        
        if (!$assignment) {
            $this->flash('danger', 'No assignment found for this member');
            $this->redirect('/groups/' . $id);
            return;
        }
        
        // Send email
        $emailService = new EmailService();
        $emailService->sendAssignmentNotification($member, $assignment, $group);
        
        $this->flash('success', 'Assignment email resent to ' . $member->getEmail());
        $this->redirect('/groups/' . $id);
    }
}
