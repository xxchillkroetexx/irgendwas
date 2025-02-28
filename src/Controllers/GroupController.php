<?php

namespace SecretSanta\Controllers;
use SecretSanta\Models\Group;
use SecretSanta\Repositories\GroupRepository;
use SecretSanta\Repositories\GroupMemberRepository;
use SecretSanta\Repositories\UserRepository;
use SecretSanta\Services\EmailService;

class GroupController extends BaseController {
    private GroupRepository $groupRepository;
    private GroupMemberRepository $memberRepository;
    private UserRepository $userRepository;
    
    public function __construct() {
        parent::__construct();
        $this->groupRepository = new GroupRepository();
        $this->memberRepository = new GroupMemberRepository();
        $this->userRepository = new UserRepository();
    }
    
    /**
     * Display a list of groups the user belongs to
     */
    public function index() {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        $groups = $this->groupRepository->findByUserId($userId);
        
        // Load admin for each group
        foreach ($groups as $group) {
            $this->groupRepository->loadAdmin($group);
        }
        
        return $this->render('group/index', [
            'groups' => $groups,
            'page_title' => 'My Groups'
        ]);
    }
    
    /**
     * Show the form to create a new group
     */
    public function create() {
        $this->requireAuth();
        
        return $this->render('group/create', [
            'page_title' => 'Create New Group'
        ]);
    }
    
    /**
     * Store a newly created group
     */
    public function store() {
        $this->requireAuth();
        
        $name = $this->request->getPostParam('name');
        $description = $this->request->getPostParam('description');
        
        // Validate inputs
        $errors = [];
        
        if (empty($name)) {
            $errors['name'] = 'Group name is required';
        }
        
        if (!empty($errors)) {
            $this->session->setFlash('errors', $errors);
            $this->session->setFlash('old', $this->request->all());
            return $this->redirect('/groups/create');
        }
        
        // Create the group
        $admin = $this->userRepository->find($this->auth->userId());
        $group = $this->groupRepository->createGroup($admin, $name, $description);
        
        // Set deadline dates if provided
        $registrationDeadline = $this->request->getPostParam('registration_deadline');
        $drawDate = $this->request->getPostParam('draw_date');
        
        if (!empty($registrationDeadline)) {
            // Convert to MySQL datetime format with time
            $deadlineTimestamp = strtotime($registrationDeadline);
            if ($deadlineTimestamp) {
                $formattedDeadline = date('Y-m-d 23:59:59', $deadlineTimestamp);
                $group->setRegistrationDeadline($formattedDeadline);
            }
        }
        
        if (!empty($drawDate)) {
            // Convert to MySQL datetime format with time
            $drawTimestamp = strtotime($drawDate);
            if ($drawTimestamp) {
                $formattedDrawDate = date('Y-m-d 12:00:00', $drawTimestamp);
                $group->setDrawDate($formattedDrawDate);
            }
        }
        
        // Save the group with the updated dates
        $this->groupRepository->save($group);
        
        $this->session->setFlash('success', 'Group created successfully!');
        return $this->redirect('/groups/' . $group->getId());
    }
    
    /**
     * Display the specified group
     */
    public function show(int $id) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);
        
        if (!$group) {
            $this->session->setFlash('error', 'Group not found');
            return $this->redirect('/groups');
        }
        
        // Check if user is a member of this group
        $member = $this->memberRepository->findByGroupAndUser($id, $userId);
        if (!$member) {
            $this->session->setFlash('error', 'You are not a member of this group');
            return $this->redirect('/groups');
        }
        
        // Load relationships
        $this->groupRepository->loadAdmin($group);
        $this->groupRepository->loadMembers($group);
        
        // Check if assignments are available for this user
        $isAdmin = $group->getAdminId() === $userId;
        $assignment = null;
        
        if ($group->isDrawn()) {
            $assignmentRepository = new \SecretSanta\Repositories\GiftAssignmentRepository();
            $assignment = $assignmentRepository->findByGiverAndGroup($userId, $id);
            
            if ($assignment) {
                $receiver = $this->userRepository->find($assignment->getReceiverId());
                $assignment->setReceiver($receiver);
            }
        }
        
        return $this->render('group/show', [
            'group' => $group,
            'is_admin' => $isAdmin,
            'assignment' => $assignment,
            'page_title' => $group->getName()
        ]);
    }
    
    /**
     * Show the form for editing the specified group
     */
    public function edit(int $id) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);
        
        if (!$group) {
            $this->session->setFlash('error', 'Group not found');
            return $this->redirect('/groups');
        }
        
        // Only admin can edit group
        if ($group->getAdminId() !== $userId) {
            $this->session->setFlash('error', 'You are not authorized to edit this group');
            return $this->redirect('/groups/' . $id);
        }
        
        return $this->render('group/edit', [
            'group' => $group,
            'page_title' => 'Edit Group: ' . $group->getName()
        ]);
    }
    
    /**
     * Update the specified group
     */
    public function update(int $id) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);
        
        if (!$group) {
            $this->session->setFlash('error', 'Group not found');
            return $this->redirect('/groups');
        }
        
        // Only admin can update group
        if ($group->getAdminId() !== $userId) {
            $this->session->setFlash('error', 'You are not authorized to edit this group');
            return $this->redirect('/groups/' . $id);
        }
        
        $name = $this->request->getPostParam('name');
        $description = $this->request->getPostParam('description');
        $registrationDeadline = $this->request->getPostParam('registration_deadline');
        $drawDate = $this->request->getPostParam('draw_date');
        
        // Validate inputs
        $errors = [];
        
        if (empty($name)) {
            $errors['name'] = 'Group name is required';
        }
        
        if (!empty($errors)) {
            $this->session->setFlash('errors', $errors);
            $this->session->setFlash('old', $this->request->all());
            return $this->redirect('/groups/' . $id . '/edit');
        }
        
        // Update group
        $group->setName($name)
              ->setDescription($description);
        
        if (!empty($registrationDeadline)) {
            // Convert to MySQL datetime format with time
            $deadlineTimestamp = strtotime($registrationDeadline);
            if ($deadlineTimestamp) {
                $formattedDeadline = date('Y-m-d 23:59:59', $deadlineTimestamp);
                $group->setRegistrationDeadline($formattedDeadline);
            }
        }
        
        if (!empty($drawDate)) {
            // Convert to MySQL datetime format with time
            $drawTimestamp = strtotime($drawDate);
            if ($drawTimestamp) {
                $formattedDrawDate = date('Y-m-d 12:00:00', $drawTimestamp);
                $group->setDrawDate($formattedDrawDate);
            }
        }
        
        $this->groupRepository->save($group);
        
        $this->session->setFlash('success', 'Group updated successfully!');
        return $this->redirect('/groups/' . $id);
    }
    
    /**
     * Remove the group (only admin can do this)
     */
    public function delete(int $id) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);
        
        if (!$group) {
            $this->session->setFlash('error', 'Group not found');
            return $this->redirect('/groups');
        }
        
        // Only admin can delete group
        if ($group->getAdminId() !== $userId) {
            $this->session->setFlash('error', 'You are not authorized to delete this group');
            return $this->redirect('/groups/' . $id);
        }
        
        // Begin transaction to delete all related records
        $this->groupRepository->beginTransaction();
        
        try {
            // Delete group members
            $members = $this->memberRepository->findByGroupId($id);
            foreach ($members as $member) {
                $this->memberRepository->delete($member);
            }
            
            // Delete gift assignments
            $assignmentRepository = new \SecretSanta\Repositories\GiftAssignmentRepository();
            $assignments = $assignmentRepository->findByGroupId($id);
            foreach ($assignments as $assignment) {
                $assignmentRepository->delete($assignment);
            }
            
            // Delete exclusion rules if they exist
            if (class_exists('\SecretSanta\Repositories\ExclusionRuleRepository')) {
                $exclusionRepository = new \SecretSanta\Repositories\ExclusionRuleRepository();
                $exclusions = $exclusionRepository->findByGroupId($id);
                foreach ($exclusions as $exclusion) {
                    $exclusionRepository->delete($exclusion);
                }
            }
            
            // Delete the group itself
            $this->groupRepository->delete($group);
            
            $this->groupRepository->commit();
            
            $this->session->setFlash('success', 'Group deleted successfully!');
            return $this->redirect('/groups');
        } catch (\Exception $e) {
            $this->groupRepository->rollback();
            $this->session->setFlash('error', 'Failed to delete group: ' . $e->getMessage());
            return $this->redirect('/groups/' . $id);
        }
    }
    
    /**
     * Join a group using an invitation code
     */
    public function showJoin() {
        $this->requireAuth();
        
        // Check if there's a code in the query string
        $code = $this->request->getQueryParam('code');
        
        return $this->render('group/join', [
            'page_title' => 'Join a Group',
            'code' => $code
        ]);
    }
    
    /**
     * Process the group join request
     */
    public function join() {
        $this->requireAuth();
        
        $code = $this->request->getPostParam('invitation_code');
        
        if (empty($code)) {
            $this->session->setFlash('error', 'Invitation code is required');
            return $this->redirect('/groups/join');
        }
        
        $group = $this->groupRepository->findByInvitationCode($code);
        
        if (!$group) {
            $this->session->setFlash('error', 'Invalid invitation code');
            return $this->redirect('/groups/join');
        }
        
        $userId = $this->auth->userId();
        
        // Check if already a member
        $existingMember = $this->memberRepository->findByGroupAndUser($group->getId(), $userId);
        
        if ($existingMember) {
            $this->session->setFlash('info', 'You are already a member of this group');
            return $this->redirect('/groups/' . $group->getId());
        }
        
        // Check if registration deadline has passed
        $deadline = $group->getRegistrationDeadline();
        if ($deadline && strtotime($deadline) < time()) {
            $this->session->setFlash('error', 'The registration deadline for this group has passed');
            return $this->redirect('/groups/join');
        }
        
        // Add user to group
        $this->memberRepository->addMember($group->getId(), $userId);
        
        $this->session->setFlash('success', 'You have successfully joined the group!');
        return $this->redirect('/groups/' . $group->getId());
    }
    
    /**
     * Leave a group
     */
    public function leave(int $id) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);
        
        if (!$group) {
            $this->session->setFlash('error', 'Group not found');
            return $this->redirect('/groups');
        }
        
        // Admin cannot leave their own group
        if ($group->getAdminId() === $userId) {
            $this->session->setFlash('error', 'As the admin, you cannot leave the group. You can delete it instead.');
            return $this->redirect('/groups/' . $id);
        }
        
        // Check if user is a member
        $member = $this->memberRepository->findByGroupAndUser($id, $userId);
        
        if (!$member) {
            $this->session->setFlash('error', 'You are not a member of this group');
            return $this->redirect('/groups');
        }
        
        // Remove user from group
        $this->memberRepository->removeMember($id, $userId);
        
        $this->session->setFlash('success', 'You have left the group');
        return $this->redirect('/groups');
    }
    
    /**
     * Perform the Secret Santa draw for a group
     */
    public function draw(int $id) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);
        
        if (!$group) {
            $this->session->setFlash('error', 'Group not found');
            return $this->redirect('/groups');
        }
        
        // Only admin can perform the draw
        if ($group->getAdminId() !== $userId) {
            $this->session->setFlash('error', 'Only the admin can perform the draw');
            return $this->redirect('/groups/' . $id);
        }
        
        // Check if already drawn
        if ($group->isDrawn()) {
            $this->session->setFlash('error', 'The draw has already been performed for this group');
            return $this->redirect('/groups/' . $id);
        }
        
        // Perform the draw
        $success = $this->groupRepository->performDraw($group);
        
        if (!$success) {
            $this->session->setFlash('error', 'Failed to perform the draw. Make sure there are enough members and the exclusion rules allow for a valid draw.');
            return $this->redirect('/groups/' . $id);
        }
        
        // Send notifications to members
        try {
            $this->sendDrawNotifications($group);
            $this->session->setFlash('success', 'The draw has been performed successfully and notifications have been sent to all members!');
        } catch (\Exception $e) {
            $this->session->setFlash('warning', 'The draw has been performed successfully, but there was an error sending notifications: ' . $e->getMessage());
        }
        
        return $this->redirect('/groups/' . $id);
    }
    
    /**
     * Generate and send a new invitation link for sharing
     */
    public function generateInvitationLink(int $id) {
        $this->requireAuth();
        
        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);
        
        if (!$group) {
            $this->session->setFlash('error', 'Group not found');
            return $this->redirect('/groups');
        }
        
        // Only admin can generate new invitation links
        if ($group->getAdminId() !== $userId) {
            $this->session->setFlash('error', 'Only the admin can generate new invitation links');
            return $this->redirect('/groups/' . $id);
        }
        
        // Generate a new invitation code
        $group->setInvitationCode(substr(str_shuffle('ABCDEFGHJKLMNPQRSTUVWXYZ23456789'), 0, 8));
        $this->groupRepository->save($group);
        
        $this->session->setFlash('success', 'New invitation code generated successfully');
        return $this->redirect('/groups/' . $id);
    }
    
    /**
     * Send notifications to members after the draw
     */
    private function sendDrawNotifications(Group $group) {
        // Load necessary relationships
        $this->groupRepository->loadMembers($group);
        
        $assignmentRepository = new \SecretSanta\Repositories\GiftAssignmentRepository();
        $assignments = $assignmentRepository->findByGroupId($group->getId());
        
        $emailService = new EmailService();
        $baseUrl = $_SERVER['REQUEST_SCHEME'] . '://' . $_SERVER['HTTP_HOST'];
        
        foreach ($assignments as $assignment) {
            $giver = $this->userRepository->find($assignment->getGiverId());
            $receiver = $this->userRepository->find($assignment->getReceiverId());
            
            $assignment->setGiver($giver);
            $assignment->setReceiver($receiver);
            
            // Send email notification
            $emailService->sendDrawNotification(
                $giver->getEmail(),
                $giver->getName(),
                $receiver->getName(),
                $group->getName(),
                $baseUrl . '/groups/' . $group->getId()
            );
            
            // Mark assignment as notified
            $assignment->setNotificationSent(true);
            $assignmentRepository->save($assignment);
        }
    }
}