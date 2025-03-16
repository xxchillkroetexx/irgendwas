<?php

namespace SecretSanta\Controllers;

use SecretSanta\Models\Group;
use SecretSanta\Repositories\GroupRepository;
use SecretSanta\Repositories\GroupMemberRepository;
use SecretSanta\Repositories\UserRepository;
use SecretSanta\Services\EmailService;

/**
 * Group Controller
 * 
 * Handles all group-related operations including CRUD operations, member management,
 * invitation handling, and the Secret Santa draw process.
 * 
 * @package SecretSanta\Controllers
 */
class GroupController extends BaseController
{
    /**
     * Repository instances for data access
     * 
     * @var GroupRepository
     * @var GroupMemberRepository
     * @var UserRepository
     */
    private GroupRepository $groupRepository;
    private GroupMemberRepository $memberRepository;
    private UserRepository $userRepository;

    /**
     * Constructor - initializes repositories for data access
     */
    public function __construct()
    {
        parent::__construct();
        $this->groupRepository = new GroupRepository();
        $this->memberRepository = new GroupMemberRepository();
        $this->userRepository = new UserRepository();
    }

    /**
     * Display a list of groups the user belongs to
     * 
     * @return string HTML content
     */
    public function index()
    {
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
     * 
     * @return string HTML content
     */
    public function create()
    {
        $this->requireAuth();

        return $this->render('group/create', [
            'page_title' => 'Create New Group'
        ]);
    }

    /**
     * Store a newly created group
     * 
     * Creates a group with the authenticated user as admin
     * 
     * @return void
     */
    public function store()
    {
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

        $this->session->setFlash('success', t('flash.success.group_created'));
        return $this->redirect('/groups/' . $group->getId());
    }

    /**
     * Display the specified group
     * 
     * Shows group details and members. Includes assignment info if draw is complete.
     * 
     * @param int $id Group ID
     * @return string|void HTML content or redirect
     */
    public function show(int $id)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);

        if (!$group) {
            $this->session->setFlash('error', t('flash.error.group_not_found'));
            return $this->redirect('/groups');
        }

        // Check if user is a member of this group
        $member = $this->memberRepository->findByGroupAndUser($id, $userId);
        if (!$member) {
            $this->session->setFlash('error', t('flash.error.not_group_member'));
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
     * 
     * Only accessible to group admin
     * 
     * @param int $id Group ID
     * @return string|void HTML content or redirect
     */
    public function edit(int $id)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);

        if (!$group) {
            $this->session->setFlash('error', t('flash.error.group_not_found'));
            return $this->redirect('/groups');
        }

        // Only admin can edit group
        if ($group->getAdminId() !== $userId) {
            $this->session->setFlash('error', t('flash.error.not_authorized_edit'));
            return $this->redirect('/groups/' . $id);
        }

        return $this->render('group/edit', [
            'group' => $group,
            'page_title' => 'Edit Group: ' . $group->getName()
        ]);
    }

    /**
     * Update the specified group
     * 
     * Only accessible to group admin
     * 
     * @param int $id Group ID
     * @return void
     */
    public function update(int $id)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);

        if (!$group) {
            $this->session->setFlash('error', t('flash.error.group_not_found'));
            return $this->redirect('/groups');
        }

        // Only admin can update group
        if ($group->getAdminId() !== $userId) {
            $this->session->setFlash('error', t('flash.error.not_authorized_edit'));
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

        $this->session->setFlash('success', t('flash.success.group_updated'));
        return $this->redirect('/groups/' . $id);
    }

    /**
     * Deletes a group and all associated data including memberships,
     * assignments, and exclusion rules
     * 
     * @param int $id Group ID
     * @return void
     */
    public function delete(int $id)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);

        if (!$group) {
            $this->session->setFlash('error', t('flash.error.group_not_found'));
            return $this->redirect('/groups');
        }

        // Only admin can delete group
        if ($group->getAdminId() !== $userId) {
            $this->session->setFlash('error', t('flash.error.not_authorized_delete'));
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

            $this->session->setFlash('success', t('flash.success.group_deleted'));
            return $this->redirect('/groups');
        } catch (\Exception $e) {
            $this->groupRepository->rollback();
            $this->session->setFlash('error', t('flash.error.group_delete_failed', ['error' => $e->getMessage()]));
            return $this->redirect('/groups/' . $id);
        }
    }

    /**
     * Join a group using an invitation code
     * 
     * Displays the form to enter an invitation code
     * 
     * @return string HTML content
     */
    public function showJoin()
    {
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
     * 
     * Validates the invitation code and adds the user to the group if valid
     * 
     * @return void
     */
    public function join()
    {
        $this->requireAuth();

        $code = $this->request->getPostParam('invitation_code');

        if (empty($code)) {
            $this->session->setFlash('error', t('flash.error.invitation_code_required'));
            return $this->redirect('/groups/join');
        }

        $group = $this->groupRepository->findByInvitationCode($code);

        if (!$group) {
            $this->session->setFlash('error', t('flash.error.invalid_invitation_code'));
            return $this->redirect('/groups/join');
        }

        $userId = $this->auth->userId();

        // Check if already a member
        $existingMember = $this->memberRepository->findByGroupAndUser($group->getId(), $userId);

        if ($existingMember) {
            $this->session->setFlash('info', t('flash.info.already_member'));
            return $this->redirect('/groups/' . $group->getId());
        }

        // Check if registration deadline has passed
        $deadline = $group->getRegistrationDeadline();
        if ($deadline && strtotime($deadline) < time()) {
            $this->session->setFlash('error', t('flash.error.registration_deadline_passed'));
            return $this->redirect('/groups/join');
        }

        // Add user to group
        $this->memberRepository->addMember($group->getId(), $userId);

        $this->session->setFlash('success', t('flash.success.group_joined'));
        return $this->redirect('/groups/' . $group->getId());
    }

    /**
     * Leave a group
     * 
     * Removes the user from a group they are a member of
     * Group admin cannot leave their own group
     * 
     * @param int $id Group ID
     * @return void
     */
    public function leave(int $id)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);

        if (!$group) {
            $this->session->setFlash('error', t('flash.error.group_not_found'));
            return $this->redirect('/groups');
        }

        // Admin cannot leave their own group
        if ($group->getAdminId() === $userId) {
            $this->session->setFlash('error', t('flash.error.admin_cannot_leave'));
            return $this->redirect('/groups/' . $id);
        }

        // Check if user is a member
        $member = $this->memberRepository->findByGroupAndUser($id, $userId);

        if (!$member) {
            $this->session->setFlash('error', t('flash.error.not_group_member'));
            return $this->redirect('/groups');
        }

        // Remove user from group
        $this->memberRepository->removeMember($id, $userId);

        $this->session->setFlash('success', t('flash.success.left_group'));
        return $this->redirect('/groups');
    }

    /**
     * Perform the Secret Santa draw for a group
     * 
     * Assigns each member a recipient for gift-giving,
     * respecting any exclusion rules. Only the group admin can perform the draw.
     * 
     * @param int $id Group ID
     * @return void
     */
    public function draw(int $id)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);

        if (!$group) {
            $this->session->setFlash('error', t('flash.error.group_not_found'));
            return $this->redirect('/groups');
        }

        // Only admin can perform the draw
        if ($group->getAdminId() !== $userId) {
            $this->session->setFlash('error', t('flash.error.only_admin_draw'));
            return $this->redirect('/groups/' . $id);
        }

        // Check if already drawn
        if ($group->isDrawn()) {
            $this->session->setFlash('error', t('flash.error.already_drawn'));
            return $this->redirect('/groups/' . $id);
        }

        // Perform the draw
        $success = $this->groupRepository->performDraw($group);

        if (!$success) {
            $this->session->setFlash('error', t('flash.error.draw_failed'));
            return $this->redirect('/groups/' . $id);
        }

        // Send notifications to members
        try {
            $this->sendDrawNotifications($group);
            $this->session->setFlash('success', t('flash.success.draw_success'));
        } catch (\Exception $e) {
            $this->session->setFlash('warning', t('flash.warning.draw_partial_success', ['error' => $e->getMessage()]));
        }

        return $this->redirect('/groups/' . $id);
    }

    /**
     * Generate and send a new invitation link for sharing
     * 
     * Creates a new invitation code for the group
     * Only the group admin can generate new invitation links
     * 
     * @param int $id Group ID
     * @return void
     */
    public function generateInvitationLink(int $id)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();
        $group = $this->groupRepository->find($id);

        if (!$group) {
            $this->session->setFlash('error', t('flash.error.group_not_found'));
            return $this->redirect('/groups');
        }

        // Only admin can generate new invitation links
        if ($group->getAdminId() !== $userId) {
            $this->session->setFlash('error', t('flash.error.only_admin_invitation'));
            return $this->redirect('/groups/' . $id);
        }

        // Generate a new invitation code
        $group->setInvitationCode(substr(str_shuffle('ABCDEFGHJKLMNPQRSTUVWXYZ23456789'), 0, 8));
        $this->groupRepository->save($group);

        $this->session->setFlash('success', t('flash.success.invitation_generated'));
        return $this->redirect('/groups/' . $id);
    }

    /**
     * Send notifications to members after the draw
     * 
     * Sends emails to all members informing them of their assigned recipient
     * 
     * @param Group $group The group object with the draw completed
     * @return void
     * @throws \Exception If email sending fails
     */
    private function sendDrawNotifications(Group $group)
    {
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
