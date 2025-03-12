<?php

namespace SecretSanta\Controllers;

use SecretSanta\Models\ExclusionRule;
use SecretSanta\Repositories\ExclusionRuleRepository;
use SecretSanta\Repositories\GroupRepository;
use SecretSanta\Repositories\UserRepository;
use SecretSanta\Repositories\GroupMemberRepository;

class ExclusionController extends BaseController
{
    private ExclusionRuleRepository $exclusionRepository;
    private GroupRepository $groupRepository;
    private UserRepository $userRepository;
    private GroupMemberRepository $memberRepository;

    public function __construct()
    {
        parent::__construct();
        $this->exclusionRepository = new ExclusionRuleRepository();
        $this->groupRepository = new GroupRepository();
        $this->userRepository = new UserRepository();
        $this->memberRepository = new GroupMemberRepository();
    }

    /**
     * Display exclusion rules for a group
     */
    public function index(int $groupId)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();

        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', 'You are not a member of this group');
            return $this->redirect('/groups');
        }

        $group = $this->groupRepository->find($groupId);
        if (!$group) {
            $this->session->setFlash('error', 'Group not found');
            return $this->redirect('/groups');
        }

        // Check if the group has already been drawn
        if ($group->isDrawn()) {
            $this->session->setFlash('error', 'Exclusions cannot be modified after the draw has been performed');
            return $this->redirect('/groups/' . $groupId);
        }

        // Load group members for dropdown
        $this->groupRepository->loadMembers($group);
        $members = $group->getMembers();

        // Get user's current exclusions
        $exclusions = $this->exclusionRepository->findByUserAndGroup($userId, $groupId);

        // Load excluded user data
        foreach ($exclusions as $exclusion) {
            $this->exclusionRepository->loadExcludedUser($exclusion);
        }

        return $this->render('exclusion/index', [
            'group' => $group,
            'members' => $members,
            'exclusions' => $exclusions,
            'page_title' => 'Manage Exclusion Rules'
        ]);
    }

    /**
     * Add a new exclusion rule
     */
    public function add(int $groupId)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();
        $excludedUserId = (int) $this->request->getPostParam('excluded_user_id');

        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', 'You are not a member of this group');
            return $this->redirect('/groups');
        }

        $group = $this->groupRepository->find($groupId);
        if (!$group) {
            $this->session->setFlash('error', 'Group not found');
            return $this->redirect('/groups');
        }

        // Check if the group has already been drawn
        if ($group->isDrawn()) {
            $this->session->setFlash('error', 'Exclusions cannot be added after the draw has been performed');
            return $this->redirect('/groups/' . $groupId);
        }

        // Check if excluded user is a member of the group
        $excludedMember = $this->memberRepository->findByGroupAndUser($groupId, $excludedUserId);
        if (!$excludedMember) {
            $this->session->setFlash('error', 'Selected user is not a member of this group');
            return $this->redirect('/exclusions/' . $groupId);
        }

        // Can't exclude yourself
        if ($userId === $excludedUserId) {
            $this->session->setFlash('error', 'You cannot exclude yourself');
            return $this->redirect('/exclusions/' . $groupId);
        }

        // Add the exclusion
        $exclusion = $this->exclusionRepository->addExclusion($groupId, $userId, $excludedUserId);

        if (!$exclusion) {
            $this->session->setFlash('error', 'Failed to add exclusion rule');
            return $this->redirect('/exclusions/' . $groupId);
        }

        $this->session->setFlash('success', 'Exclusion rule added successfully');
        return $this->redirect('/exclusions/' . $groupId);
    }

    /**
     * Remove an exclusion rule
     */
    public function remove(int $groupId, int $excludedUserId)
    {
        $this->requireAuth();

        $userId = $this->auth->userId();

        // Check if the user is a member of the group
        $member = $this->memberRepository->findByGroupAndUser($groupId, $userId);
        if (!$member) {
            $this->session->setFlash('error', 'You are not a member of this group');
            return $this->redirect('/groups');
        }

        $group = $this->groupRepository->find($groupId);
        if (!$group) {
            $this->session->setFlash('error', 'Group not found');
            return $this->redirect('/groups');
        }

        // Check if the group has already been drawn
        if ($group->isDrawn()) {
            $this->session->setFlash('error', 'Exclusions cannot be removed after the draw has been performed');
            return $this->redirect('/groups/' . $groupId);
        }

        // Remove the exclusion
        $success = $this->exclusionRepository->removeExclusion($groupId, $userId, $excludedUserId);

        if (!$success) {
            $this->session->setFlash('error', 'Failed to remove exclusion rule');
            return $this->redirect('/exclusions/' . $groupId);
        }

        $this->session->setFlash('success', 'Exclusion rule removed successfully');
        return $this->redirect('/exclusions/' . $groupId);
    }
}
